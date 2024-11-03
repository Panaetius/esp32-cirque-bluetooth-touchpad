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
    gpio::PinDriver,
    i2c::{I2cConfig, I2cDriver},
    peripherals::Peripherals,
    prelude::*,
    timer::{TimerConfig, TimerDriver},
};
use tm040040::{Address, Tm040040, XYInverted};

const TRACKPAD_ID: u8 = 0x01;

const HID_REPORT_DESCRIPTOR: &[u8] = hid!(
    (USAGE_PAGE, 0x01), // Generic Desktop
    (USAGE, 0x02),      // Mouse
    (COLLECTION, 0x01), // Application
    (USAGE, 0x01),      //Pointer
    (COLLECTION, 0x00), //Physical
    (REPORT_ID, TRACKPAD_ID),
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

const ACCEL_CURVE_SPEEDS: [f32; 4] = [7.0, 15.0, 40.0, 128.0];
const ACCEL_CURVE_FACTORS: [f32; 4] = [1.0, 1.5, 4.0, 4.0];

fn accelerate_move(x_delta: f32, y_delta: f32) -> (f32, f32) {
    let speed = (x_delta.powi(2) + y_delta.powi(2)).sqrt();
    for i in 0..3 {
        if speed >= ACCEL_CURVE_SPEEDS[i] && speed < ACCEL_CURVE_SPEEDS[i + 1] {
            let pos = (speed - ACCEL_CURVE_SPEEDS[i])
                / (ACCEL_CURVE_SPEEDS[i + 1] - ACCEL_CURVE_SPEEDS[i]);
            let factor = pos * (ACCEL_CURVE_FACTORS[i + 1] - ACCEL_CURVE_FACTORS[i])
                + ACCEL_CURVE_FACTORS[i];
            return (x_delta * factor, y_delta * factor);
        }
    }
    (x_delta, y_delta)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PowerMode {
    Active,
    Sleep,
}

#[allow(dead_code)]
#[repr(packed)]
struct MouseReport {
    buttons: u8, // bits for buttons are packed into a single u8, lowest bit = left, second lowest bit = right click
    axis: [i8; 3],
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

    let sda = peripherals.pins.gpio10;
    let scl = peripherals.pins.gpio8;
    let mut hardware_ready_pin = PinDriver::input(peripherals.pins.gpio1)?;
    let config = I2cConfig::new().baudrate(400.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, sda, scl, &config)?;
    let mut trackpad = Tm040040::new(i2c, Address::Primary, &mut hardware_ready_pin)
        .enable()
        .unwrap();
    let device_id = trackpad.device_id().unwrap();
    log::info!("Device Id:{:?}", device_id);
    trackpad.set_power_mode(tm040040::PowerMode::Sleep).unwrap();
    delay::FreeRtos::delay_ms(100);
    let power_mode = trackpad.power_mode().unwrap();
    log::info!("Powermode: {:?}", power_mode);

    let position_mode = trackpad.position_mode().unwrap();
    log::info!("Positionmode: {:?}", position_mode);
    trackpad.set_xy_inverted(XYInverted::YInverted).unwrap();

    BLEDevice::set_device_name("Awesome BT Trackpad").unwrap();

    let ble_device = BLEDevice::take();
    ble_device
        .security()
        .set_auth(AuthReq::all())
        .set_passkey(123456)
        .set_io_cap(SecurityIOCap::DisplayOnly)
        .resolve_rpa();

    let server = ble_device.get_server();
    server.on_connect(|_server, desc| {
        log::info!("Client connected:{:?}", desc);
    });
    server.on_disconnect(|desc, reason| {
        log::info!("Client disconnected:{:?}, {:?}", desc, reason);
    });

    server.on_authentication_complete(|desc, result| {
        log::info!("Auth Complete: {:?}: {:?}", result, desc);
    });
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
        .scan_response(false)
        .set_data(
            BLEAdvertisementData::new()
                .name("Awesome BT Trackpad")
                .appearance(0x03C2)
                .add_service_uuid(hid_device.hid_service().lock().uuid()),
        )
        .unwrap();
    ble_advertising.lock().on_complete(|_| {
        ble_advertising.lock().start().unwrap();
    });
    ble_advertising.lock().start().unwrap();

    let mut conn_updated = 0;
    let mut power_mode = PowerMode::Sleep;
    let mut waiting_for_sleep = false;
    let timer_tick_hz = timer1.tick_hz();
    loop {
        log::info!("Checking connections");
        log::info!("bonded addresses: {:?}", ble_device.bonded_addresses());
        while server.connected_count() > 0 {
            let connection = server.connections().next().unwrap();
            log::info!("connection:{:?}", connection);
            if !connection.bonded() {
                delay::FreeRtos::delay_ms(1000);
                continue;
            }
            if conn_updated < 10 {
                let conn_handle = server.connections().next().unwrap().conn_handle();
                server
                    .update_conn_params(conn_handle, 6, 6, 15, 50)
                    .unwrap();
                Ets::delay_ms(1);
                log::info!("connection params updated");
                conn_updated += 1;
            }
            let pad_data = trackpad.relative_data().unwrap();
            if let Some(touch_data) = pad_data {
                power_mode = PowerMode::Active;
                waiting_for_sleep = false;
                let buttons = (touch_data.primary_pressed as u8)
                    | ((touch_data.secondary_pressed as u8) << 1);
                let delta = accelerate_move(touch_data.x_delta as f32, touch_data.y_delta as f32);
                input_position
                    .lock()
                    .set_from(&MouseReport {
                        buttons,
                        axis: [
                            // accel_data.x.clamp(-126.0, 126.0) as i8,
                            // -accel_data.y.clamp(-126.0, 126.0) as i8,
                            // offset.x.clamp(i8::MIN as f32, i8::MAX as f32) as i8,
                            // (-offset.y).clamp(i8::MIN as f32, i8::MAX as f32) as i8,
                            delta.0.max(i8::MIN as f32).min(i8::MAX as f32) as i8,
                            delta.1.max(i8::MIN as f32).min(i8::MAX as f32) as i8,
                            0,
                        ],
                    })
                    .notify();
                log::info!(
                    "pad data:{:?}, scaled:{:?}",
                    (touch_data.x_delta, touch_data.y_delta),
                    (
                        delta.0.max(i8::MIN as f32).min(i8::MAX as f32) as i8,
                        delta.1.max(i8::MIN as f32).min(i8::MAX as f32) as i8
                    )
                );
            } else if power_mode == PowerMode::Active && !waiting_for_sleep {
                // No touch. we want to throttle after 5s to consume less power
                timer1.set_counter(0_u64).unwrap();
                timer1.enable(true).unwrap();
                waiting_for_sleep = true;
            } else if power_mode == PowerMode::Active
                && timer1.counter().unwrap() / timer_tick_hz > 4
            {
                // no touch for 5 seconds, enter sleep mode
                power_mode = PowerMode::Sleep;
                timer1.enable(false).unwrap();
                log::info!("entered sleep mode");
            }

            if power_mode == PowerMode::Active {
                Ets::delay_us(1);
            } else {
                delay::FreeRtos::delay_ms(100); // only check every 100ms if we are in sleep mode, hopefully saving power
            }
        }
        // let pad_data = trackpad.relative_data().unwrap();
        // log::info!("pad data:{:?}", pad_data);
        conn_updated = 0;
        delay::FreeRtos::delay_ms(1000);
    }
}
