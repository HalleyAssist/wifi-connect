use clap::{App, Arg};
use pad::{PadStr, Alignment};

use std::env;
use std::net::Ipv4Addr;
use errors::*;
use std::str::FromStr;

const DEFAULT_GATEWAY: &str = "192.168.42.1";
const DEFAULT_DHCP_RANGE: &str = "192.168.42.2,192.168.42.254";
const DEFAULT_ACTIVITY_TIMEOUT: &str = "0";
const DEFAULT_LISTENING: &str = "0.0.0.0:80";

#[derive(Clone)]
pub struct Config {
    pub interface: Option<String>,
    pub ssid: String,
    pub passphrase: Option<String>,
    pub gateway: Ipv4Addr,
    pub dhcp_range: String,
    pub listening_at: String,
    pub activity_timeout: u64,
}


pub fn get_config() -> Result<Config> {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("portal-interface")
                .short("i")
                .long("portal-interface")
                .value_name("interface")
                .help("Wireless network interface to be used by WiFi Connect")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("portal-ssid")
                .short("s")
                .long("portal-ssid")
                .value_name("ssid")
                .help("SSID of the captive portal WiFi network")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("portal-passphrase")
                .short("p")
                .long("portal-passphrase")
                .value_name("passphrase")
                .help("WPA2 Passphrase of the captive portal WiFi network (default: none)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("portal-gateway")
                .short("g")
                .long("portal-gateway")
                .value_name("gateway")
                .help(&format!(
                    "Gateway of the captive portal WiFi network (default: {})",
                    DEFAULT_GATEWAY
                ))
                .takes_value(true),
        )
        .arg(
            Arg::with_name("portal-dhcp-range")
                .short("d")
                .long("portal-dhcp-range")
                .value_name("dhcp_range")
                .help(&format!(
                    "DHCP range of the WiFi network (default: {})",
                    DEFAULT_DHCP_RANGE
                ))
                .takes_value(true),
        )
        .arg(
            Arg::with_name("portal-listening")
                .short("o")
                .long("portal-listening")
                .value_name("listening_at")
                .help(&format!(
                    "Listening address of the captive portal web server (default: {})",
                    DEFAULT_LISTENING
                ))
                .takes_value(true),
        )
        .arg(
            Arg::with_name("activity-timeout")
                .short("a")
                .long("activity-timeout")
                .value_name("activity_timeout")
                .help("Exit if no activity for the specified time (seconds) (default: none)")
                .takes_value(true),
        )
        .get_matches();

    let interface: Option<String> = matches.value_of("portal-interface").map_or_else(
        || env::var("PORTAL_INTERFACE").ok(),
        |v| Some(v.to_string())
    );

    let ssidSuffix: String;
    if let Ok(ssidResult) = &env::var("BALENA_DEVICE_UUID") {
        ssidSuffix = ssidResult[0..12].to_string()
    } else {
        ssidSuffix = env::var("RESIN_DEVICE_UUID").chain_err(|| "unable to find UUID")?[0..12].to_string();
    }
    
    let ssid = format!("HalleyHub-{}", ssidSuffix);

    let passphrase: Option<String> = Some(env::var("PAIRING_CODE").chain_err(|| "unable to find pairing code")?.pad(8, '_', Alignment::Right, false));

    let gateway = Ipv4Addr::from_str(&matches.value_of("portal-gateway").map_or_else(
        || env::var("PORTAL_GATEWAY").unwrap_or_else(|_| DEFAULT_GATEWAY.to_string()),
        String::from,
    )).expect("Cannot parse gateway address");

    let dhcp_range = matches.value_of("portal-dhcp-range").map_or_else(
        || env::var("PORTAL_DHCP_RANGE").unwrap_or_else(|_| DEFAULT_DHCP_RANGE.to_string()),
        String::from,
    );

    let listening_at = matches
        .value_of("portal-listening")
        .map_or_else(
            || {
                env::var("PORTAL_LISTENING")
                    .unwrap_or_else(|_| DEFAULT_LISTENING.to_string())
            },
            String::from,
        );

    let activity_timeout = u64::from_str(&matches.value_of("activity-timeout").map_or_else(
        || env::var("ACTIVITY_TIMEOUT").unwrap_or_else(|_| DEFAULT_ACTIVITY_TIMEOUT.to_string()),
        String::from,
    )).expect("Cannot parse activity timeout");

    Ok(Config {
        interface: interface,
        ssid: ssid,
        passphrase: passphrase,
        gateway: gateway,
        dhcp_range: dhcp_range,
        listening_at: listening_at,
        activity_timeout: activity_timeout
    })
}
