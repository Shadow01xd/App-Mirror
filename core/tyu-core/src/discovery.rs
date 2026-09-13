use crate::{CoreError, Result, TyuErrorCode};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use tyu_types::{DeviceId, ProtocolVersion};
pub const SERVICE: &str = "_tyu._udp.local.";
pub struct Discovery {
    pub daemon: ServiceDaemon,
    fullname: String,
}
impl Discovery {
    pub fn start(id: DeviceId, port: u16, capabilities: usize) -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| CoreError::Transport(e.to_string()))?;
        let info = advertisement(id, port, capabilities)?;
        let fullname = info.get_fullname().to_owned();
        daemon
            .register(info)
            .map_err(|e| CoreError::Transport(e.to_string()))?;
        Ok(Self { daemon, fullname })
    }
    pub fn stop(&self) -> Result<()> {
        self.daemon
            .unregister(&self.fullname)
            .map_err(|e| CoreError::Transport(e.to_string()))?;
        self.daemon
            .stop_browse(SERVICE)
            .map_err(|e| CoreError::Transport(e.to_string()))?;
        self.daemon
            .shutdown()
            .map_err(|e| CoreError::Transport(e.to_string()))?;
        Ok(())
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        let _result = self.stop();
    }
}
pub fn advertisement(id: DeviceId, port: u16, capabilities: usize) -> Result<ServiceInfo> {
    if port == 0 || capabilities > 32 {
        return Err(TyuErrorCode::Limit.into());
    }
    let props = HashMap::from([
        ("v".to_string(), "1.0".to_string()),
        ("id".to_string(), id.to_string()),
        ("caps".to_string(), capabilities.to_string()),
    ]);
    ServiceInfo::new(
        SERVICE,
        &id.to_string(),
        &format!("{id}.local."),
        "",
        port,
        props,
    )
    .map(|s| s.enable_addr_auto())
    .map_err(|e| CoreError::Transport(e.to_string()))
}
pub fn parse_properties(version: &str, id: &str, port: u16) -> Result<DeviceId> {
    let (major, minor) = version.split_once('.').ok_or(TyuErrorCode::Malformed)?;
    tyu_protocol::negotiate(ProtocolVersion {
        major: major.parse().map_err(|_| TyuErrorCode::Malformed)?,
        minor: minor.parse().map_err(|_| TyuErrorCode::Malformed)?,
    })?;
    if port == 0 {
        return Err(TyuErrorCode::Malformed.into());
    }
    Ok(DeviceId(id.parse().map_err(|_| TyuErrorCode::Malformed)?))
}
