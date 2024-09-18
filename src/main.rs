use esp32_nimble::{
    enums::{AuthReq, SecurityIOCap},
    hid::{
        hid, COLLECTION, END_COLLECTION, HIDINPUT, LOGICAL_MAXIMUM, LOGICAL_MINIMUM, REPORT_COUNT,
        REPORT_ID, REPORT_SIZE, USAGE, USAGE_MAXIMUM, USAGE_MINIMUM, USAGE_PAGE,
    },
    BLEAdvertisementData, BLEDevice, BLEHIDDevice,
};
use esp_idf_hal::delay::{self, Ets};

const TRACKPAD_ID: u8 = 0x01;

const HID_REPORT_DESCRIPTOR: &[u8] = hid!(
    (USAGE_PAGE, 0x01), // Generic Desktop
    (USAGE, 0x02),      // Mouse
    (COLLECTION, 0x01), // Application
    (REPORT_ID, TRACKPAD_ID),
    (USAGE, 0x01),      //Pointer
    (COLLECTION, 0x00), //Physical
    //---------------------------- Mouse buttons --------------
    (USAGE_PAGE, 0x09),    //Button
    (USAGE_MINIMUM, 0x01), //Button1
    (USAGE_MAXIMUM, 0x02), //Button2
    (LOGICAL_MINIMUM, 0x00),
    (LOGICAL_MAXIMUM, 0x01),
    (REPORT_SIZE, 0x01),
    (REPORT_COUNT, 0x02),
    (HIDINPUT, 0x02),
    //---------------------------- Padding --------------------
    (REPORT_SIZE, 0x06),
    (REPORT_COUNT, 0x01),
    (HIDINPUT, 0x03),
    //---------------------------- Mouse Position -------------
    (USAGE_PAGE, 0x01),      //generic desktop
    (USAGE, 0x30),           //X
    (USAGE, 0x31),           //Y
    (USAGE, 0x38),           //wheel
    (LOGICAL_MINIMUM, 0x81), //-127
    (LOGICAL_MAXIMUM, 0x7f), //127
    (REPORT_SIZE, 0x08),
    (REPORT_COUNT, 0x03),
    (HIDINPUT, 0x06),
    //---------------------------- Footer ---------------------
    (END_COLLECTION),
    (END_COLLECTION)
);

#[repr(packed)]
struct MouseReport {
    buttons: u8, // bits for buttons are packed into a single u8, lowest bit = left, second lowest bit = right click
    axis: [i8; 3],
}

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    BLEDevice::set_device_name("Awesome BT Trackpad").unwrap();

    let ble_device = BLEDevice::take();
    ble_device
        .security()
        .set_auth(AuthReq::all())
        .set_passkey(123456)
        .set_io_cap(SecurityIOCap::NoInputNoOutput)
        .resolve_rpa();

    let server = ble_device.get_server();
    let mut hid_device = BLEHIDDevice::new(server);
    hid_device.manufacturer("Hogru");
    hid_device.pnp(0x01, 0x0000, 0x0001, 0x0100);
    hid_device.hid_info(0x00, 0x01);
    hid_device.report_map(HID_REPORT_DESCRIPTOR);
    hid_device.set_battery_level(100);

    let input_position = hid_device.input_report(TRACKPAD_ID);

    let ble_advertising = ble_device.get_advertising();
    ble_advertising
        .lock()
        .set_data(
            BLEAdvertisementData::new()
                .name("Awesome BT Trackpad")
                .appearance(0x03C2)
                .add_service_uuid(hid_device.hid_service().lock().uuid()),
        )
        .unwrap();
    ble_advertising.lock().start().unwrap();

    server.on_authentication_complete(|desc, result| {
        log::info!("Auth Complete: {:?}: {:?}", result, desc);
    });

    loop {
        delay::FreeRtos::delay_ms(1000);
        log::info!("Checking connections");
        if server.connected_count() > 0 {
            log::info!("connected!");
            input_position
                .lock()
                .set_from(&MouseReport {
                    buttons: 0,
                    axis: [25, 25, 0],
                })
                .notify();
            Ets::delay_ms(7);
        }
    }
}
