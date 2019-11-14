use std::sync::mpsc::{Receiver, Sender};
use std::fmt;
use std::net::Ipv4Addr;
use std::error::Error as StdError;

use serde_json;
use iron::prelude::*;
use iron::{headers, status, typemap, AfterMiddleware, Iron, IronError, IronResult, Request,
           Response, Url};
use router::Router;
use persistent::Write;
use params::{FromValue, Params};

use errors::*;
use network::{NetworkCommand, NetworkCommandResponse};
use exit::{exit, ExitResult};

struct RequestSharedState {
    server_rx: Receiver<NetworkCommandResponse>,
    network_tx: Sender<NetworkCommand>,
    exit_tx: Sender<ExitResult>,
}

impl typemap::Key for RequestSharedState {
    type Value = RequestSharedState;
}

#[derive(Debug)]
struct StringError(String);

impl fmt::Display for StringError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl StdError for StringError {
    fn description(&self) -> &str {
        &*self.0
    }
}

macro_rules! get_request_ref {
    ($req:ident, $ty:ty, $err:expr) => (
        match $req.get_ref::<$ty>() {
            Ok(val) => val,
            Err(err) => {
                error!($err);
                return Err(IronError::new(err, status::InternalServerError));
            }
        }
    )
}

macro_rules! get_param {
    ($params:ident, $param:expr, $ty:ty) => (
        match $params.get($param) {
            Some(value) => {
                match <$ty as FromValue>::from_value(value) {
                    Some(converted) => converted,
                    None => {
                        let err = format!("Unexpected type for '{}'", $param);
                        error!("{}", err);
                        return Err(IronError::new(StringError(err), status::InternalServerError));
                    }
                }
            },
            None => {
                let err = format!("'{}' not found in request params: {:?}", $param, $params);
                error!("{}", err);
                return Err(IronError::new(StringError(err), status::InternalServerError));
            }
        }
    )
}

macro_rules! get_request_state {
    ($req:ident) => (
        get_request_ref!(
            $req,
            Write<RequestSharedState>,
            "Getting reference to request shared state failed"
        ).as_ref().lock().unwrap()
    )
}

fn output_error(e_kind: ErrorKind) -> IronResult<Response>
{
    let description = e_kind.description().into();
    Err(IronError::new(
        StringError(description),
        status::InternalServerError,
    ))
}

fn exit_with_error<E>(state: &RequestSharedState, e: E, e_kind: ErrorKind) -> IronResult<Response>
where
    E: ::std::error::Error + Send + 'static,
{
    let description = e_kind.description().into();
    let err = Err::<Response, E>(e).chain_err(|| e_kind);
    exit(&state.exit_tx, err.unwrap_err());
    Err(IronError::new(
        StringError(description),
        status::InternalServerError,
    ))
}

pub fn start_server(
    gateway: Ipv4Addr,
    address: String,
    server_rx: Receiver<NetworkCommandResponse>,
    network_tx: Sender<NetworkCommand>,
    exit_tx: Sender<ExitResult>
) {
    let exit_tx_clone = exit_tx.clone();
    let request_state = RequestSharedState {
        gateway: gateway,
        server_rx: server_rx,
        network_tx: network_tx,
        exit_tx: exit_tx,
    };

    let mut router = Router::new();
    router.get("/networks", networks, "networks");
    router.post("/connect", connect, "connect");
    router.get("/enable_ap", enable_ap, "enable_ap");
    router.get("/disable_ap", disable_ap, "disable_ap");
    router.get("/restart_ap", restart_ap, "restart_ap");
    router.get("/current", current, "current");
    router.get("/has_connection", has_connection, "has_connection");


    let mut chain = Chain::new(router);
    chain.link(Write::<RequestSharedState>::both(request_state));

    info!("Starting HTTP server on {}", &address);

    if let Err(e) = (Iron { handler: chain, threads: 1,  timeouts: iron::Timeouts::default() }).http(&address) {
        exit(
            &exit_tx_clone,
            ErrorKind::StartHTTPServer(address, e.description().into()).into(),
        );
    }
}

fn networks(req: &mut Request) -> IronResult<Response> {
    info!("User connected to the captive portal");

    let request_state = get_request_state!(req);

    if let Err(e) = request_state.network_tx.send(NetworkCommand::Activate) {
        return exit_with_error(&request_state, e, ErrorKind::SendNetworkCommandActivate);
    }

    let networks = match request_state.server_rx.recv() {
        Ok(result) => match result {
            NetworkCommandResponse::Networks(networks) => networks,
            _ => return output_error(ErrorKind::IncorrectCommand),
        },
        Err(e) => return exit_with_error(&request_state, e, ErrorKind::RecvAccessPointSSIDs),
    };

    let access_points_json = match serde_json::to_string(&networks) {
        Ok(json) => json,
        Err(e) => return exit_with_error(&request_state, e, ErrorKind::SerializeAccessPointSSIDs),
    };

    Ok(Response::with((status::Ok, access_points_json)))
}

fn connect(req: &mut Request) -> IronResult<Response> {
    let (ssid, identity, passphrase) = {
        let params = get_request_ref!(req, Params, "Getting request params failed");
        let ssid = get_param!(params, "ssid", String);
        let identity = get_param!(params, "identity", String);
        let passphrase = get_param!(params, "passphrase", String);
        (ssid, identity, passphrase)
    };

    debug!("Incoming `connect` to access point `{}` request", ssid);

    let request_state = get_request_state!(req);

    let command = NetworkCommand::Connect {
        ssid: ssid,
        identity: identity,
        passphrase: passphrase,
    };

    if let Err(e) = request_state.network_tx.send(command) {
        exit_with_error(&request_state, e, ErrorKind::SendNetworkCommandConnect)
    } else {
        Ok(Response::with(status::Ok))
    }
}

fn enable_ap(req: &mut Request) -> IronResult<Response> {
    debug!("Incoming `enable_ap` to access point");

    let request_state = get_request_state!(req);
    
    let command = NetworkCommand::EnableAp {};

    if let Err(e) = request_state.network_tx.send(command) {
        exit_with_error(&request_state, e, ErrorKind::SendNetworkCommandEnableAp)
    } else {
        Ok(Response::with(status::Ok))
    }
}

fn disable_ap(req: &mut Request) -> IronResult<Response> {
    debug!("Incoming `disable_ap` to access point");

    let request_state = get_request_state!(req);

    let command = NetworkCommand::DisableAp {};

    if let Err(e) = request_state.network_tx.send(command) {
        exit_with_error(&request_state, e, ErrorKind::SendNetworkCommandDisableAp)
    } else {
        Ok(Response::with(status::Ok))
    }
}

fn restart_ap(req: &mut Request) -> IronResult<Response> {
    debug!("Incoming `restart_ap` to access point");

    let request_state = get_request_state!(req);

    let command1 = NetworkCommand::DisableAp {};

    if let Err(e) = request_state.network_tx.send(command1) {
        exit_with_error(&request_state, e, ErrorKind::SendNetworkCommandDisableAp);
    }
    
    let command = NetworkCommand::EnableAp {};

    if let Err(e) = request_state.network_tx.send(command) {
        exit_with_error(&request_state, e, ErrorKind::SendNetworkCommandEnableAp)
    } else {
        Ok(Response::with(status::Ok))
    }
}

fn current(req: &mut Request) -> IronResult<Response> {
    let request_state = get_request_state!(req);

    if let Err(e) = request_state.network_tx.send(NetworkCommand::Current) {
        return exit_with_error(&request_state, e, ErrorKind::SendNetworkCommandCurrent);
    }

    let state = match request_state.server_rx.recv() {
        Ok(result) => match result {
            NetworkCommandResponse::Current(state) => state,
            _ => return output_error(ErrorKind::IncorrectCommand),
        },
        Err(e) => return exit_with_error(&request_state, e, ErrorKind::RecvAccessPointSSIDs),
    };

    let state_json = match serde_json::to_string(&state) {
        Ok(json) => json,
        Err(e) => return exit_with_error(&request_state, e, ErrorKind::SerializeAccessPointSSIDs),
    };

    Ok(Response::with((status::Ok, state_json)))

}

fn has_connection(req: &mut Request) -> IronResult<Response> {
    let request_state = get_request_state!(req);

    if let Err(e) = request_state.network_tx.send(NetworkCommand::HasConnection) {
        return exit_with_error(&request_state, e, ErrorKind::SendNetworkCommandHasConnection);
    }

    let state = match request_state.server_rx.recv() {
        Ok(result) => match result {
            NetworkCommandResponse::HasConnection(state) => state,
            _ => return output_error(ErrorKind::IncorrectCommand),
        },
        Err(e) => return exit_with_error(&request_state, e, ErrorKind::RecvAccessPointSSIDs),
    };

    let state_json = match serde_json::to_string(&state) {
        Ok(json) => json,
        Err(e) => return exit_with_error(&request_state, e, ErrorKind::SerializeAccessPointSSIDs),
    };

    Ok(Response::with((status::Ok, state_json)))

}