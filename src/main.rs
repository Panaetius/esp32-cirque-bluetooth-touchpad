use esp32_nimble::{uuid128, BLEAdvertisementData, BLEDevice, NimbleProperties};
use esp_idf_hal::delay;

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let ble_device = BLEDevice::take();
    let server = ble_device.get_server();
    server.on_connect(|server, desc| {
        log::info!("client connected");
        server
            .update_conn_params(desc.conn_handle(), 24, 48, 0, 60)
            .unwrap();
        log::info!("multiconnect: start advertising");
        ble_device.get_advertising().lock().start().unwrap();
    });
    let service = server.create_service(uuid128!("fafafafa-fafa-fafa-fafa-fafafafafafa"));
    let static_characteristics = service.lock().create_characteristic(
        uuid128!("d4e0e0d0-1a2b-11e9-ab14-d663bd873d93"),
        NimbleProperties::READ,
    );
    static_characteristics
        .lock()
        .set_value("hellow world!".as_bytes());
    let notifying_characteristic = service.lock().create_characteristic(
        uuid128!("a3c87500-8ed3-4bdf-8a39-a01bebede295"),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    notifying_characteristic
        .lock()
        .set_value("initial".as_bytes());

    let ble_advertising = ble_device.get_advertising();
    ble_advertising
        .lock()
        .set_data(
            BLEAdvertisementData::new()
                .name("BT Trackpad")
                .add_service_uuid(uuid128!("68f41eac-ee28-11ec-8ea0-0242ac120002")),
        )
        .unwrap();
    ble_advertising.lock().start().unwrap();
    let mut counter = 0;
    loop {
        delay::FreeRtos::delay_ms(1000);
        notifying_characteristic
            .lock()
            .set_value(format!("Counter: {counter}").as_bytes())
            .notify();
        counter += 1;
    }
}
