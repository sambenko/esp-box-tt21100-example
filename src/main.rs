#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_println::println;
use core::cell::RefCell;
use critical_section::Mutex;

use display_interface_spi::SPIInterfaceNoCS;
use embedded_graphics::{
    pixelcolor::Rgb565, prelude::*, 
};

use hal::{
    clock::{ClockControl, CpuClock},
    i2c::I2C,
    spi::{master::Spi, SpiMode},
    gpio::{ Event, GpioPin, Input, PullUp },
    peripherals::{ Peripherals, Interrupt, I2C0 },
    prelude::{_fugit_RateExtU32, *},
    IO, Delay,
    interrupt,
};

use tt21100::TT21100;

static TOUCH_CONTROLLER: Mutex<RefCell<Option<TT21100<I2C<I2C0>, GpioPin<Input<PullUp>, 3>>>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let peripherals = Peripherals::take();

    let system = peripherals.SYSTEM.split();
    let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock240MHz).freeze();

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut delay = Delay::new(&clocks);

    let sclk = io.pins.gpio7;
    let mosi = io.pins.gpio6;

    let dc = io.pins.gpio4.into_push_pull_output();
    let mut backlight = io.pins.gpio45.into_push_pull_output();
    let reset = io.pins.gpio48.into_push_pull_output();

    let spi = Spi::new_no_cs_no_miso(
        peripherals.SPI2,
        sclk,
        mosi,
        60u32.MHz(),
        SpiMode::Mode0,
        &clocks,
    );

    let di = SPIInterfaceNoCS::new(spi, dc);
    delay.delay_ms(500u32);

    let mut display = match mipidsi::Builder::ili9342c_rgb565(di)
        .with_display_size(320, 240)
        .with_orientation(mipidsi::Orientation::PortraitInverted(false))
        .with_color_order(mipidsi::ColorOrder::Bgr)
        .init(&mut delay, Some(reset)) {
        Ok(display) => display,
        Err(e) => {
            println!("Display initialization failed: {:?}", e);
            panic!("Display initialization failed");
        }
    };

    backlight.set_high().unwrap();

    display.clear(Rgb565::WHITE).unwrap();

    let i2c = I2C::new(
        peripherals.I2C0,
        io.pins.gpio8,
        io.pins.gpio18,
        100u32.kHz(),
        &clocks,
    );

    let mut irq_pin = io.pins.gpio3.into_pull_up_input();
    irq_pin.listen(Event::RisingEdge);

    let touch_controller = TT21100::new(i2c, irq_pin).expect("Failed to initialize touch controller");
    
    critical_section::with(|cs| TOUCH_CONTROLLER.borrow_ref_mut(cs).replace(touch_controller));

    interrupt::enable(Interrupt::GPIO, interrupt::Priority::Priority1)
        .expect("Invalid Interrupt Priority Error");
    
    loop {}
}

#[ram]
#[interrupt]
fn GPIO() {
    critical_section::with(|cs| {
        if let Some(touch_controller) = TOUCH_CONTROLLER.borrow_ref_mut(cs).as_mut() {
            if touch_controller.data_available().unwrap_or(false) {
                match touch_controller.event() {
                    Ok(event) => match event {
                        tt21100::Event::Touch { report, touches } => {
                            println!("Touch Event: Report ID: {}", report.report_id);
                            if let Some(touch) = touches.0 {
                                println!("Touch 1: X: {}, Y: {}, pressure: {}", touch.x, touch.y, touch.pressure);
                            }
                            if let Some(touch) = touches.1 {
                                println!("Touch 2: X: {}, Y: {}, pressure: {}", touch.x, touch.y, touch.pressure);
                            }
                        }
                        tt21100::Event::Button(button) => {
                            println!("Button Event: Report ID: {}", button.report_id);
                        }
                    },
                    Err(e) => println!("Error reading touch event: {:?}", e),
                }
            }
        }
    });
}