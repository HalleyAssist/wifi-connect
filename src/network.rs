use std::thread;
use std::process;
use std::time::Duration;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::error::Error;
use std::net::Ipv4Addr;

use network_manager::{AccessPoint, AccessPointCredentials, Connection, ConnectionState,
                      Connectivity, Device, DeviceType, DeviceState, NetworkManager, Security, ServiceState};

use errors::*;
use exit::{exit, trap_exit_signals, ExitResult};
use config::Config;
use dnsmasq::start_dnsmasq;
use server::start_server;

use std::rc::Rc;

#[derive(Clone)]
struct AP {
    ap: Rc<AccessPoint>,
}

pub enum NetworkCommand {
    EnableAp,
    DisableAp,
    Current,
    HasConnection,
    Activate,
    Timeout,
    Exit,
    Connect {
        ssid: String,
        identity: String,
        passphrase: String,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Network {
    ssid: String,
    security: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CurrentStatus {
    apmode: bool,
    connected: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct HasConnection {
    result: bool
}

pub enum NetworkCommandResponse {
    Networks(Vec<Network>),
    Current(CurrentStatus),
    HasConnection(HasConnection),
}

struct NetworkCommandHandler {
    manager: NetworkManager,
    device: Device,
    portal_connection: Option<Connection>,
    config: Config,
    access_points: Vec<AP>,
    dnsmasq: Option<process::Child>,
    server_tx: Sender<NetworkCommandResponse>,
    network_rx: Receiver<NetworkCommand>,
    activated: bool,
}

impl NetworkCommandHandler {
    fn new(config: &Config, exit_tx: &Sender<ExitResult>) -> Result<Self> {
        let (network_tx, network_rx) = channel();

        Self::spawn_trap_exit_signals(exit_tx, network_tx.clone());

        let manager = NetworkManager::new();
        debug!("NetworkManager connection initialized");

        let device = find_device(&manager, &config.interface)?;

        let dnsmasq;
        let portal_connection;

        if has_connection_defined()? == false {
            portal_connection = Some(create_portal(&device, &config)?);
            dnsmasq = Some(start_dnsmasq(&config, &device)?);
        } else {
            portal_connection = None;
            dnsmasq = None;
        }

        if let Some(wifi_device) = device.as_wifi_device() {
            let _ = wifi_device.request_scan();
        }
        thread::sleep(Duration::from_secs(4));

        let access_points = get_access_points(&device, &config.ssid)?;

        let (server_tx, server_rx) = channel();

        Self::spawn_server(config, exit_tx, server_rx, network_tx.clone());

        Self::spawn_activity_timeout(config, network_tx.clone());

        let config = config.clone();
        let activated = false;

        Ok(NetworkCommandHandler {
            manager,
            device,
            config,
            access_points,
            dnsmasq,
            portal_connection,
            server_tx,
            network_rx,
            activated,
        })
    }

    fn spawn_server(
        config: &Config,
        exit_tx: &Sender<ExitResult>,
        server_rx: Receiver<NetworkCommandResponse>,
        network_tx: Sender<NetworkCommand>,
    ) {
        let gateway = config.gateway;
        let listening_at = config.listening_at.clone();
        let exit_tx_server = exit_tx.clone();
        let ui_directory = config.ui_directory.clone();

        thread::spawn(move || {
            start_server(
                gateway,
                listening_at,
                server_rx,
                network_tx,
                exit_tx_server,
                &ui_directory,
            );
        });
    }

    fn spawn_activity_timeout(config: &Config, network_tx: Sender<NetworkCommand>) {
        let activity_timeout = config.activity_timeout;

        if activity_timeout == 0 {
            return;
        }

        thread::spawn(move || {
            thread::sleep(Duration::from_secs(activity_timeout));

            if let Err(err) = network_tx.send(NetworkCommand::Timeout) {
                error!(
                    "Sending NetworkCommand::Timeout failed: {}",
                    err.description()
                );
            }
        });
    }

    fn spawn_trap_exit_signals(exit_tx: &Sender<ExitResult>, network_tx: Sender<NetworkCommand>) {
        let exit_tx_trap = exit_tx.clone();

        thread::spawn(move || {
            if let Err(e) = trap_exit_signals() {
                exit(&exit_tx_trap, e);
                return;
            }

            if let Err(err) = network_tx.send(NetworkCommand::Exit) {
                error!("Sending NetworkCommand::Exit failed: {}", err.description());
            }
        });
    }

    fn run(&mut self, exit_tx: &Sender<ExitResult>) {
        let result = self.run_loop();
        self.stop(exit_tx, result);
    }

    fn run_loop(&mut self) -> ExitResult {
        loop {
            let command = self.receive_network_command()?;

            match command {
                NetworkCommand::EnableAp => {
                    if self.portal_connection.is_none() {
                        let wifi_device = self.device.as_wifi_device().unwrap();
                        if let Ok(_) = wifi_device.request_scan()  {
                            thread::sleep(Duration::from_secs(4));
                        }

                        self.portal_connection = Some(create_portal(&self.device, &self.config)?);
                        self.dnsmasq = Some(start_dnsmasq(&self.config, &self.device)?);
                    }
                },
                NetworkCommand::DisableAp => {
                    self._stop();
                },
                NetworkCommand::Current => {
                    self.current()?;
                },
                NetworkCommand::HasConnection => {
                    self.has_connection()?;
                },
                NetworkCommand::Activate => {
                    self.activate()?;
                },
                NetworkCommand::Timeout => {
                    if !self.activated {
                        info!("Timeout reached. Exiting...");
                        return Ok(());
                    }
                },
                NetworkCommand::Exit => {
                    info!("Exiting...");
                    return Ok(());
                },
                NetworkCommand::Connect {
                    ssid,
                    identity,
                    passphrase,
                } => {
                    if self.connect(&ssid, &identity, &passphrase)? {
                        return Ok(());
                    }
                },
            }
        }
    }

    fn receive_network_command(&self) -> Result<NetworkCommand> {
        match self.network_rx.recv() {
            Ok(command) => Ok(command),
            Err(e) => {
                // Sleep for a second, so that other threads may log error info.
                thread::sleep(Duration::from_secs(1));
                Err(e).chain_err(|| ErrorKind::RecvNetworkCommand)
            },
        }
    }

    fn _stop(&mut self) {
        if let Some(ref mut dnsmasq) = self.dnsmasq {
            let _ = dnsmasq.kill();
        }
        self.dnsmasq = None;

        if let Some(ref connection) = self.portal_connection {
            let _ = stop_portal_impl(connection, &self.config);
        }
        self.portal_connection = None;
    }

    fn stop(&mut self, exit_tx: &Sender<ExitResult>, result: ExitResult) {
        self._stop();

        let _ = exit_tx.send(result);
    }

    fn current(&mut self) -> ExitResult {
        let state = self.device.get_state()?;

        let status = CurrentStatus {
            apmode: self.portal_connection.is_none(),
            connected: state == DeviceState::Activated
        };

        self.server_tx
            .send(NetworkCommandResponse::Current(status))
            .chain_err(|| ErrorKind::SendStatus)
    }

    fn has_connection(&mut self) -> ExitResult {
        let status = HasConnection {
            result: has_connection_defined()?
        };

        self.server_tx
            .send(NetworkCommandResponse::HasConnection(status))
            .chain_err(|| ErrorKind::SendHasConnection)
    }

    
    fn get_access_points(&mut self) -> Result<Vec<AP>> {
        let mut new_access_points = get_access_points(&self.device, &self.config.ssid)?;

        for x in &self.access_points {
            let xssid = x.ap.ssid().as_str().unwrap();
            if let Some(_) = new_access_points.iter().find(|xx| xx.ap.ssid().as_str().unwrap() == xssid) {
                new_access_points.push((*x).clone());
            }
        }

        return Ok(new_access_points);
    }

    fn activate(&mut self) -> ExitResult {
        self.activated = true;

        let access_points = self.get_access_points()?;
        let networks = get_networks(&access_points);

        self.server_tx
            .send(NetworkCommandResponse::Networks(networks))
            .chain_err(|| ErrorKind::SendAccessPointSSIDs)
    }

    fn connect(&mut self, ssid: &str, identity: &str, passphrase: &str) -> Result<bool> {
        delete_connection_if_exists(&self.manager, ssid);

        if let Some(ref connection) = self.portal_connection {
            stop_portal(connection, &self.config)?;
        }

        self.portal_connection = None;

        let access_points = self.get_access_points()?;

        if let Some(access_point) = find_access_point(&access_points, ssid) {
            let wifi_device = self.device.as_wifi_device().unwrap();

            info!("Connecting to access point '{}'...", ssid);

            let credentials = init_access_point_credentials(access_point, identity, passphrase);

            match wifi_device.connect(access_point, &credentials) {
                Ok((connection, state)) => {
                    if state == ConnectionState::Activated {
                        match wait_for_connectivity(&self.manager, 20) {
                            Ok(has_connectivity) => {
                                if has_connectivity {
                                    info!("Internet connectivity established");
                                } else {
                                    warn!("Cannot establish Internet connectivity");
                                }
                            },
                            Err(err) => error!("Getting Internet connectivity failed: {}", err),
                        }

                        return Ok(true);
                    }

                    if let Err(err) = connection.delete() {
                        error!("Deleting connection object failed: {}", err)
                    }

                    warn!(
                        "Connection to access point not activated '{}': {:?}",
                        ssid, state
                    );
                },
                Err(e) => {
                    warn!("Error connecting to access point '{}': {}", ssid, e);
                },
            }
        }

        self.portal_connection = Some(create_portal(&self.device, &self.config)?);

        Ok(false)
    }
}

fn init_access_point_credentials(
    access_point: &AccessPoint,
    identity: &str,
    passphrase: &str,
) -> AccessPointCredentials {
    if access_point.security.contains(Security::ENTERPRISE) {
        AccessPointCredentials::Enterprise {
            identity: identity.to_string(),
            passphrase: passphrase.to_string(),
        }
    } else if access_point.security.contains(Security::WPA2)
        || access_point.security.contains(Security::WPA)
    {
        AccessPointCredentials::Wpa {
            passphrase: passphrase.to_string(),
        }
    } else if access_point.security.contains(Security::WEP) {
        AccessPointCredentials::Wep {
            passphrase: passphrase.to_string(),
        }
    } else {
        AccessPointCredentials::None
    }
}

pub fn process_network_commands(config: &Config, exit_tx: &Sender<ExitResult>) {
    let mut command_handler = match NetworkCommandHandler::new(config, exit_tx) {
        Ok(command_handler) => command_handler,
        Err(e) => {
            exit(exit_tx, e);
            return;
        },
    };

    command_handler.run(exit_tx);
}

pub fn init_networking() -> Result<()> {
    start_network_manager_service()?;

    delete_access_point_profiles().chain_err(|| ErrorKind::DeleteAccessPoint)
}

pub fn find_device(manager: &NetworkManager, interface: &Option<String>) -> Result<Device> {
    if let Some(ref interface) = *interface {
        let device = manager
            .get_device_by_interface(interface)
            .chain_err(|| ErrorKind::DeviceByInterface(interface.clone()))?;

        if *device.device_type() == DeviceType::WiFi {
            info!("Targeted WiFi device: {}", interface);
            Ok(device)
        } else {
            bail!(ErrorKind::NotAWiFiDevice(interface.clone()))
        }
    } else {
        let devices = manager.get_devices()?;

        let index = devices
            .iter()
            .position(|d| *d.device_type() == DeviceType::WiFi);

        if let Some(index) = index {
            info!("WiFi device: {}", devices[index].interface());
            Ok(devices[index].clone())
        } else {
            bail!(ErrorKind::NoWiFiDevice)
        }
    }
}

fn get_access_points(device: &Device, own_ssid: &str) -> Result<Vec<AP>> {
    get_access_points_impl(device, own_ssid).chain_err(|| ErrorKind::NoAccessPoints)
}

fn get_access_points_impl(device: &Device, own_ssid: &str) -> Result<Vec<AP>> {
    let retries_allowed = 10;
    let mut retries = 0;

    let wifi_device = device.as_wifi_device().unwrap();
    if let Ok(_) = wifi_device.request_scan() {
        thread::sleep(Duration::from_secs(4));
    }

    // After stopping the hotspot we may have to wait a bit for the list
    // of access points to become available
    while retries < retries_allowed {
        let mut access_points = wifi_device.get_access_points()?;

        access_points.retain(|ap| ap.ssid().as_str().is_ok() && ap.ssid().as_str().unwrap() != own_ssid);

        if !access_points.is_empty() {
            info!(
                "Access points: {:?}",
                get_access_points_ssids(&access_points)
            );
            let ap = access_points.into_iter().map(|x| AP {ap: Rc::new(x)} ).collect();
            return Ok(ap);
        }

        retries += 1;
        debug!("No access points found - retry #{}", retries);
        thread::sleep(Duration::from_secs(1));
    }

    warn!("No access points found - giving up...");
    Ok(vec![])
}

fn get_access_points_ssids(access_points: &[AccessPoint]) -> Vec<&str> {
    access_points
        .iter()
        .map(|ap| ap.ssid().as_str().unwrap())
        .collect()
}

fn get_networks(access_points: &[AP]) -> Vec<Network> {
    access_points
        .iter()
        .map(|ap| get_network_info(ap.ap))
        .collect()
}

fn get_network_info(access_point: std::rc::Rc<network_manager::AccessPoint>) -> Network {
    Network {
        ssid: access_point.ssid().as_str().unwrap().to_string(),
        security: get_network_security(access_point).to_string(),
    }
}

fn get_network_security(access_point: &AccessPoint) -> &str {
    if access_point.security.contains(Security::ENTERPRISE) {
        "enterprise"
    } else if access_point.security.contains(Security::WPA2)
        || access_point.security.contains(Security::WPA)
    {
        "wpa"
    } else if access_point.security.contains(Security::WEP) {
        "wep"
    } else {
        "none"
    }
}

fn find_access_point<'a>(access_points: &'a [AP], ssid: &str) -> Option<std::rc::Rc<network_manager::AccessPoint>> {
    for access_point in access_points.iter() {
        if let Ok(access_point_ssid) = access_point.ap.ssid().as_str() {
            if access_point_ssid == ssid {
                return Some(access_point.ap);
            }
        }
    }

    None
}

fn create_portal(device: &Device, config: &Config) -> Result<Connection> {
    let portal_passphrase = config.passphrase.as_ref().map(|p| p as &str);

    create_portal_impl(device, &config.ssid, &config.gateway, &portal_passphrase)
        .chain_err(|| ErrorKind::CreateCaptivePortal)
}

fn create_portal_impl(
    device: &Device,
    ssid: &str,
    gateway: &Ipv4Addr,
    passphrase: &Option<&str>,
) -> Result<Connection> {
    info!("Starting access point...");
    let wifi_device = device.as_wifi_device().unwrap();
    let (portal_connection, _) = wifi_device.create_hotspot(ssid, *passphrase, Some(*gateway))?;
    info!("Access point '{}' created with passphrase '{}'", ssid, passphrase.unwrap_or_default());
    Ok(portal_connection)
}

fn stop_portal(connection: &Connection, config: &Config) -> Result<()> {
    stop_portal_impl(connection, config).chain_err(|| ErrorKind::StopAccessPoint)
}

fn stop_portal_impl(connection: &Connection, config: &Config) -> Result<()> {
    info!("Stopping access point '{}'...", config.ssid);
    connection.deactivate()?;
    connection.delete()?;
    thread::sleep(Duration::from_secs(1));
    info!("Access point '{}' stopped", config.ssid);
    Ok(())
}

fn wait_for_connectivity(manager: &NetworkManager, timeout: u64) -> Result<bool> {
    let mut total_time = 0;

    loop {
        let connectivity = manager.get_connectivity()?;

        if connectivity == Connectivity::Full || connectivity == Connectivity::Limited {
            debug!(
                "Connectivity established: {:?} / {}s elapsed",
                connectivity, total_time
            );

            return Ok(true);
        } else if total_time >= timeout {
            debug!(
                "Timeout reached in waiting for connectivity: {:?} / {}s elapsed",
                connectivity, total_time
            );

            return Ok(false);
        }

        ::std::thread::sleep(::std::time::Duration::from_secs(1));

        total_time += 1;

        debug!(
            "Still waiting for connectivity: {:?} / {}s elapsed",
            connectivity, total_time
        );
    }
}

pub fn start_network_manager_service() -> Result<()> {
    let state = match NetworkManager::get_service_state() {
        Ok(state) => state,
        _ => {
            info!("Cannot get the NetworkManager service state");
            return Ok(());
        },
    };

    if state != ServiceState::Active {
        let state = NetworkManager::start_service(15).chain_err(|| ErrorKind::StartNetworkManager)?;
        if state != ServiceState::Active {
            bail!(ErrorKind::StartActiveNetworkManager);
        } else {
            info!("NetworkManager service started successfully");
        }
    } else {
        debug!("NetworkManager service already running");
    }

    Ok(())
}

pub fn has_connection_defined() -> Result<bool> {
    let manager = NetworkManager::new();

    let connections = manager.get_connections()?;

    for connection in connections {
        if &connection.settings().kind == "802-11-wireless" && &connection.settings().mode != "ap" {
            return Ok(true);
        }
    }

    return Ok(false);
}

fn delete_access_point_profiles() -> Result<()> {
    let manager = NetworkManager::new();

    let connections = manager.get_connections()?;

    for connection in connections {
        if &connection.settings().kind == "802-11-wireless" && &connection.settings().mode == "ap" {
            debug!(
                "Deleting access point connection profile: {:?}",
                connection.settings().ssid,
            );
            connection.delete()?;
        }
    }

    Ok(())
}

fn delete_connection_if_exists(manager: &NetworkManager, ssid: &str) {
    let connections = match manager.get_connections() {
        Ok(connections) => connections,
        Err(e) => {
            error!("Getting existing connections failed: {}", e);
            return;
        },
    };

    for connection in connections {
        if let Ok(connection_ssid) = connection.settings().ssid.as_str() {
            if &connection.settings().kind == "802-11-wireless" && connection_ssid == ssid {
                info!(
                    "Deleting existing WiFi connection: {:?}",
                    connection.settings().ssid,
                );

                if let Err(e) = connection.delete() {
                    error!("Deleting existing WiFi connection failed: {}", e);
                }
            }
        }
    }
}
