use clap::{Parser, Subcommand, ValueEnum};
use hypernova_ctl::device::HypernovaDevice;
use hypernova_ctl::protocol::{Lod, PollingRate, RgbColor, SensorMode};
use hypernova_ctl::transport::HidApiTransport;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "hypernova-ctl", version, about = "Configure Cosmic Byte Hypernova gaming mouse")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read connection state, firmware, battery, and all sensor settings.
    Status,
    /// Configure DPI stages, active stage, and stage count.
    #[command(subcommand)]
    Dpi(DpiCommand),
    /// Set USB polling rate in Hz (125, 250, 500, 1000, 2000, 4000).
    Rate { hz: u16 },
    /// Set lift-off distance (1mm or 2mm).
    Lod { height: LodArg },
    /// Set button debounce time in ms (0..=30).
    Debounce { ms: u8 },
    /// Set PAW3395 sensor power mode (lp or hp).
    SensorMode { mode: SensorModeArg },
    /// Toggle Motion Sync (on or off).
    MotionSync { state: ToggleArg },
    /// Toggle Ripple Control (on or off).
    Ripple { state: ToggleArg },
    /// Toggle Angle Snapping / linear correction (on or off).
    AngleSnapping { state: ToggleArg },
    /// Set custom sleep timer in seconds (30..=2400 in 10s steps) or 'off'.
    Sleep { value: String },
    /// Restore factory default settings on the mouse.
    Reset,
}

#[derive(Subcommand)]
enum DpiCommand {
    /// Set DPI (and optional Y DPI / indicator color) for a 1-based stage (1..=6).
    Set {
        stage: u8,
        dpi: u16,
        #[arg(long)]
        y: Option<u16>,
        #[arg(long, value_parser = parse_hex_rgb)]
        color: Option<RgbColor>,
    },
    /// Switch active DPI stage (1..=6).
    Active { stage: u8 },
    /// Set number of enabled DPI stages (1..=6).
    Stages { count: u8 },
}

#[derive(Clone, Copy, ValueEnum)]
enum LodArg {
    #[value(name = "1mm")]
    Mm1,
    #[value(name = "2mm")]
    Mm2,
}

#[derive(Clone, Copy, ValueEnum)]
enum SensorModeArg {
    Lp,
    Hp,
}

#[derive(Clone, Copy, ValueEnum)]
enum ToggleArg {
    On,
    Off,
}

fn parse_hex_rgb(input: &str) -> Result<RgbColor, String> {
    let hex = input.strip_prefix('#').unwrap_or(input);
    if hex.len() != 6 {
        return Err(format!("expected 6-digit hex color like FF00AA, got '{input}'"));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
    Ok(RgbColor { r, g, b })
}

fn parse_sleep_arg(input: &str) -> Result<Option<u16>, String> {
    if input.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    let secs: u16 = input
        .parse()
        .map_err(|_| format!("expected 'off' or seconds (30..=2400), got '{input}'"))?;
    Ok(Some(secs))
}

fn print_status(dev: &HypernovaDevice<HidApiTransport>) -> Result<(), String> {
    let online = dev.is_online().map_err(|e| e.to_string())?;
    println!("Connection:     {}", dev.connection_kind());
    println!("Mouse Online:   {}", if online { "yes" } else { "no (move mouse to wake)" });
    if !online {
        return Ok(());
    }

    let fw = dev.firmware_version().map_err(|e| e.to_string())?;
    let batt = dev.battery().map_err(|e| e.to_string())?;
    let cfg = dev.read_config().map_err(|e| e.to_string())?;

    println!("Firmware:       0x{fw:04X}");
    println!(
        "Battery:        {}% ({} mV{})",
        batt.percent,
        batt.voltage_mv,
        if batt.charging { ", charging" } else { "" }
    );
    println!("Polling Rate:   {}", cfg.polling_rate);
    println!("Sensor Mode:    {}", cfg.sensor_mode);
    println!("LOD:            {}", cfg.lod);
    println!("Debounce:       {} ms", cfg.debounce_ms);
    println!("Motion Sync:    {}", on_off(cfg.motion_sync));
    println!("Ripple Control: {}", on_off(cfg.ripple_control));
    println!("Angle Snapping: {}", on_off(cfg.angle_snapping));
    if cfg.custom_sleep_enabled {
        println!("Sleep Timer:    {} s", cfg.sleep_seconds);
    } else {
        println!("Sleep Timer:    default (custom off)");
    }
    println!(
        "DPI Stages ({} enabled, active: {}):",
        cfg.max_dpi_stages,
        cfg.active_dpi_stage + 1
    );
    for i in 0..usize::from(cfg.max_dpi_stages.min(6)) {
        let stage = cfg.dpi_stages[i];
        let marker = if i == usize::from(cfg.active_dpi_stage) { "*" } else { " " };
        if stage.x == stage.y {
            println!("  {marker} Stage {}: {:>5} DPI  {}", i + 1, stage.x, stage.color);
        } else {
            println!(
                "  {marker} Stage {}: {:>5}x{:<5} DPI  {}",
                i + 1,
                stage.x,
                stage.y,
                stage.color
            );
        }
    }
    Ok(())
}

fn on_off(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

fn run(cli: Cli) -> Result<(), String> {
    let transport = HidApiTransport::open().map_err(|e| e.to_string())?;
    let dev = HypernovaDevice::new(transport);

    match cli.command {
        Command::Status => print_status(&dev)?,
        Command::Dpi(DpiCommand::Set { stage, dpi, y, color }) => {
            let y_dpi = y.unwrap_or(dpi);
            dev.set_dpi_stage(stage, dpi, y_dpi).map_err(|e| e.to_string())?;
            if let Some(rgb) = color {
                dev.set_dpi_color(stage, rgb).map_err(|e| e.to_string())?;
            }
            println!("Stage {stage} set to {dpi}x{y_dpi} DPI");
        }
        Command::Dpi(DpiCommand::Active { stage }) => {
            dev.set_active_dpi_stage(stage).map_err(|e| e.to_string())?;
            println!("Active DPI stage set to {stage}");
        }
        Command::Dpi(DpiCommand::Stages { count }) => {
            dev.set_max_dpi_stages(count).map_err(|e| e.to_string())?;
            println!("Enabled DPI stages set to {count}");
        }
        Command::Rate { hz } => {
            let rate = PollingRate::from_hz(hz).map_err(|e| e.to_string())?;
            dev.set_polling_rate(rate).map_err(|e| e.to_string())?;
            println!("Polling rate set to {rate}");
        }
        Command::Lod { height } => {
            let lod = match height {
                LodArg::Mm1 => Lod::Mm1,
                LodArg::Mm2 => Lod::Mm2,
            };
            dev.set_lod(lod).map_err(|e| e.to_string())?;
            println!("LOD set to {lod}");
        }
        Command::Debounce { ms } => {
            dev.set_debounce_ms(ms).map_err(|e| e.to_string())?;
            println!("Debounce set to {ms} ms");
        }
        Command::SensorMode { mode } => {
            let m = match mode {
                SensorModeArg::Lp => SensorMode::LowPower,
                SensorModeArg::Hp => SensorMode::HighPerformance,
            };
            dev.set_sensor_mode(m).map_err(|e| e.to_string())?;
            println!("Sensor mode set to {m}");
        }
        Command::MotionSync { state } => {
            let on = matches!(state, ToggleArg::On);
            dev.set_motion_sync(on).map_err(|e| e.to_string())?;
            println!("Motion Sync set to {}", on_off(on));
        }
        Command::Ripple { state } => {
            let on = matches!(state, ToggleArg::On);
            dev.set_ripple_control(on).map_err(|e| e.to_string())?;
            println!("Ripple Control set to {}", on_off(on));
        }
        Command::AngleSnapping { state } => {
            let on = matches!(state, ToggleArg::On);
            dev.set_angle_snapping(on).map_err(|e| e.to_string())?;
            println!("Angle Snapping set to {}", on_off(on));
        }
        Command::Sleep { value } => {
            let secs = parse_sleep_arg(&value)?;
            dev.set_sleep_time(secs).map_err(|e| e.to_string())?;
            match secs {
                Some(s) => println!("Custom sleep timer set to {s} s"),
                None => println!("Custom sleep timer disabled"),
            }
        }
        Command::Reset => {
            dev.factory_reset().map_err(|e| e.to_string())?;
            println!("Factory defaults restored");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}
