use anyhow::Result;
use esp32_nimble::{
    enums::{AuthReq, SecurityIOCap},
    hid::{
        hid, COLLECTION, END_COLLECTION, HIDINPUT, LOGICAL_MAXIMUM, LOGICAL_MINIMUM, REPORT_COUNT,
        REPORT_ID, REPORT_SIZE, USAGE, USAGE_MAXIMUM, USAGE_MINIMUM, USAGE_PAGE,
    },
    BLEAdvertisementData, BLEDevice, BLEHIDDevice,
};
use esp_idf_hal::{
    delay::{self, Ets},
    i2c::{I2cConfig, I2cDriver},
    peripherals::Peripherals,
    prelude::*,
    timer::{TimerConfig, TimerDriver},
};
use icm42670::{
    accelerometer::vector::{F32x3, I16x3},
    Address, Icm42670, PowerMode,
};

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

fn to_i8(v: i16) -> i8 {
    if v > i8::MAX as i16 {
        i8::MAX
    } else if v < i8::MIN as i16 {
        i8::MIN
    } else {
        v as i8
    }
}

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let config = TimerConfig::new();
    let mut timer1 = TimerDriver::new(peripherals.timer00, &config).unwrap();
    timer1.set_counter(0u64).unwrap();
    timer1.enable(true).unwrap();

    let sda = peripherals.pins.gpio10;
    let scl = peripherals.pins.gpio8;
    let config = I2cConfig::new().baudrate(400.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, sda, scl, &config)?;
    let mut imu = Icm42670::new(i2c, Address::Primary).unwrap();
    let device_id = imu.device_id().unwrap();
    log::info!("Device Id:{:?}", device_id);

    imu.set_power_mode(PowerMode::GyroLowNoise).unwrap();
    imu.set_gyro_odr(icm42670::GyroOdr::Hz1600).unwrap();
    // imu.set_gyro_lp_filter_bandwidth(GyroLpFiltBw::Hz180)
    //     .unwrap();

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
    let mut delay_index = 0;
    let delays = [1, 5, 10, 20];
    let mut last_values: Option<I16x3> = None;
    let mut conn_updated = 0;
    let mut prev_time = timer1.counter().unwrap();

    loop {
        log::info!("Checking connections");
        while server.connected_count() > 0 {
            if conn_updated < 100 {
                let conn_handle = server.connections().next().unwrap().conn_handle();
                server.update_conn_params(conn_handle, 6, 6, 0, 50).unwrap();
                Ets::delay_ms(1);
                log::info!("connection params updated");
                conn_updated += 1;
            }
            // log::info!("connected!");
            // log::info!("delay: {:?}", delays[delay_index]);
            // for i in 0..12 {
            //     input_position
            //         .lock()
            //         .set_from(&MouseReport {
            //             buttons: 0,
            //             axis: [i * 10, 0, 0],
            //         })
            //         .notify();
            //     Ets::delay_ms(delays[delay_index]);
            // }
            // delay::FreeRtos::delay_ms(5000);
            // delay_index = (delay_index + 1) % 4;

            // let mut offset: F32x3 = F32x3::new(0.0, 0.0, 0.0);
            // let mut accel_data = F32x3::new(0.0, 0.0, 0.0);
            // for _ in 0..1 {
            //     let data = imu.gyro_norm().unwrap();
            //     accel_data = F32x3::new(
            //         accel_data.x + data.x,
            //         accel_data.y + data.y,
            //         accel_data.z + data.z,
            //     );
            // }
            // accel_data = F32x3::new(
            //     accel_data.x / 12.0,
            //     accel_data.y / 12.0,
            //     accel_data.z / 12.0,
            // );
            let accel_data = imu.gyro_raw().unwrap();
            if last_values.is_none() {
                last_values = Some(accel_data);
            } else if let Some(vals) = last_values {
                // let offset = F32x3::new(
                //     accel_data.x - vals.x,
                //     accel_data.y - vals.y,
                //     accel_data.z - vals.z,
                // );
                // prev_time = timer1.counter().unwrap();
                input_position
                    .lock()
                    .set_from(&MouseReport {
                        buttons: 0,
                        axis: [
                            // accel_data.x.clamp(-126.0, 126.0) as i8,
                            // -accel_data.y.clamp(-126.0, 126.0) as i8,
                            // offset.x.clamp(i8::MIN as f32, i8::MAX as f32) as i8,
                            // (-offset.y).clamp(i8::MIN as f32, i8::MAX as f32) as i8,
                            (accel_data.x - vals.x)
                                .max(i8::MAX as i16)
                                .min(i8::MIN as i16) as i8,
                            (vals.x - accel_data.x)
                                .max(i8::MAX as i16)
                                .min(i8::MIN as i16) as i8,
                            0,
                        ],
                    })
                    .notify();
                // let new_time = timer1.counter().unwrap();

                log::info!("gyro data: {:?}", imu.gyro_raw().unwrap());
            }
            Ets::delay_us(1);
        }
        conn_updated = 0;
        last_values = None;
        delay::FreeRtos::delay_ms(1000);
    }
}
