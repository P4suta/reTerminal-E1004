use device_runtime::{
    measurements::IoFailure,
    wifi::{Network, WifiFactory, WifiSession, MAX_NETWORKS},
};
use esp_radio::wifi::{
    scan::ScanConfig, sta::StationConfig, AuthenticationMethodConfig, Config, WifiController,
};

struct Factory(esp_hal::peripherals::WIFI<'static>);
struct Session<'a>(WifiController<'a>);
impl WifiFactory for Factory {
    type Session<'a> = Session<'a>;
    fn start(&mut self) -> Result<Session<'_>, IoFailure> {
        WifiController::new(self.0.reborrow(), Default::default())
            .map(Session)
            .map_err(|_| IoFailure::Bus)
    }
}
impl WifiSession for Session<'_> {
    async fn scan(&mut self) -> Result<heapless::Vec<Network, MAX_NETWORKS>, IoFailure> {
        let found = self
            .0
            .scan_async(&ScanConfig::default().with_max(MAX_NETWORKS))
            .await
            .map_err(|_| IoFailure::Bus)?;
        let mut networks = heapless::Vec::new();
        for network in found {
            let ssid = network
                .ssid
                .as_str()
                .try_into()
                .map_err(|_| IoFailure::Bus)?;
            networks
                .push(Network {
                    ssid,
                    signal_dbm: network.signal_strength,
                })
                .map_err(|_| IoFailure::Bus)?;
        }
        Ok(networks)
    }
    async fn connect(&mut self, ssid: &str, password: &str) -> Result<(), IoFailure> {
        let authentication = if password.is_empty() {
            AuthenticationMethodConfig::Open
        } else {
            AuthenticationMethodConfig::Wpa2Personal(
                password.try_into().map_err(|_| IoFailure::Bus)?,
            )
        };
        let config = Config::Station(
            StationConfig::default()
                .with_ssid(ssid.try_into().map_err(|_| IoFailure::Bus)?)
                .with_authentication(authentication),
        );
        self.0.set_config(&config).map_err(|_| IoFailure::Bus)?;
        self.0
            .connect_async()
            .await
            .map(|_| ())
            .map_err(|_| IoFailure::Bus)
    }
    async fn disconnect(&mut self) -> Result<(), IoFailure> {
        self.0
            .disconnect_async()
            .await
            .map(|_| ())
            .map_err(|_| IoFailure::Bus)
    }
}
#[embassy_executor::task]
pub async fn task(wifi: esp_hal::peripherals::WIFI<'static>) {
    device_runtime::wifi::wifi_loop(&crate::SYSTEM, Factory(wifi)).await;
}
