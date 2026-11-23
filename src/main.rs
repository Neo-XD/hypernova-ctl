use clap::{Parser, Subcommand, ValueEnum};
use hypernova_ctl::device::{HypernovaDevice, PollingRateStep};
use hypernova_ctl::protocol::{Lod, PollingRate, RgbColor, SensorMode};
use hypernova_ctl::transport::{HidApiTransport, TransportError};
use std::process::ExitCode;
use std::time::Duration;

const WAKE_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Parser)]
#[command(name = "hypernova-ctl", version, about = "Configure Cosmic Byte Hypernova gaming mouse")]
struct Cli {
    /// Seconds to wait for a sleeping wireless mouse to wake (0 to fail immediately).
    #[arg(long, global = true, default_value_t = 5)]
    wait: u64,
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
    /// Read or set USB polling rate (<hz>, up, down, cycle, max; omit to read current).
    Rate {
        #[arg(value_parser = parse_rate_action)]
        action: Option<RateAction>,
    },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RateAction {
    Exact(PollingRate),
    Up,
    Down,
    Cycle,
    Max,
}

fn parse_rate_action(input: &str) -> Result<RateAction, String> {
    if input.eq_ignore_ascii_case("up") {
        return Ok(RateAction::Up);
    }
    if input.eq_ignore_ascii_case("down") {
        return Ok(RateAction::Down);
    }
    if input.eq_ignore_ascii_case("cycle") {
        return Ok(RateAction::Cycle);
    }
    if input.eq_ignore_ascii_case("max") {
        return Ok(RateAction::Max);
    }
    let hz: u16 = input
        .parse()
        .map_err(|_| format!("expected Hz (125..4000) or up/down/cycle/max, got '{input}'"))?;
    PollingRate::from_hz(hz)
        .map(RateAction::Exact)
        .map_err(|e| e.to_string())
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

fn ensure_awake(dev: &HypernovaDevice<HidApiTransport>, wait_secs: u64) -> Result<(), String> {
    dev.wait_until_online(Duration::from_secs(wait_secs), WAKE_POLL_INTERVAL, || {
        eprintln!("Mouse is asleep. Move your mouse within {wait_secs}s to apply...");
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn print_status(dev: &HypernovaDevice<HidApiTransport>, wait_secs: u64) -> Result<(), String> {
    println!("Connection:     {}", dev.connection_kind());
    let online = match dev.wait_until_online(
        Duration::from_secs(wait_secs),
        WAKE_POLL_INTERVAL,
        || {
            eprintln!("Mouse is asleep. Move your mouse within {wait_secs}s to wake...");
        },
    ) {
        Ok(_) => true,
        Err(TransportError::DeviceOffline) => false,
        Err(e) => return Err(e.to_string()),
    };
    println!(
        "Mouse Online:   {}",
        if online {
            "yes"
        } else {
            "no (move mouse to wake)"
        }
    );
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
    if cfg.polling_rate.manages_sensor_mode_internally() {
        println!(
            "Sensor Mode:    {} (firmware-managed at {})",
            cfg.sensor_mode, cfg.polling_rate
        );
    } else {
        println!("Sensor Mode:    {}", cfg.sensor_mode);
    }
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

fn format_supported_rates(rates: &[PollingRate]) -> String {
    rates
        .iter()
        .map(|r| r.hz().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn handle_rate(
    dev: &HypernovaDevice<HidApiTransport>,
    action: Option<RateAction>,
) -> Result<(), String> {
    match action {
        None => {
            let rate = dev.read_polling_rate().map_err(|e| e.to_string())?;
            let supported = format_supported_rates(dev.connection_kind().supported_polling_rates());
            println!("Polling Rate:   {rate}");
            println!("Connection:     {}", dev.connection_kind());
            println!("Supported:      {supported} Hz");
        }
        Some(RateAction::Exact(rate)) => {
            dev.set_polling_rate(rate).map_err(|e| e.to_string())?;
            println!("Polling rate set to {rate}");
        }
        Some(RateAction::Max) => {
            let max = dev.connection_kind().max_polling_rate();
            dev.set_polling_rate(max).map_err(|e| e.to_string())?;
            println!("Polling rate set to {max}");
        }
        Some(step_action) => {
            let step = match step_action {
                RateAction::Up => PollingRateStep::Up,
                RateAction::Down => PollingRateStep::Down,
                RateAction::Cycle => PollingRateStep::Cycle,
                RateAction::Exact(_) | RateAction::Max => unreachable!(),
            };
            let (prev, next) = dev.step_polling_rate(step).map_err(|e| e.to_string())?;
            if prev == next {
                println!("Polling rate already at {next}");
            } else {
                println!("Polling rate changed from {prev} to {next}");
            }
        }
    }
    Ok(())
}

fn run(cli: Cli) -> Result<(), String> {
    let transport = HidApiTransport::open().map_err(|e| e.to_string())?;
    let dev = HypernovaDevice::new(transport);

    if matches!(cli.command, Command::Status) {
        return print_status(&dev, cli.wait);
    }
    ensure_awake(&dev, cli.wait)?;

    match cli.command {
        Command::Status => unreachable!(),
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
        Command::Rate { action } => handle_rate(&dev, action)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rate_action_accepts_hz_and_keywords() {
        assert_eq!(parse_rate_action("125").unwrap(), RateAction::Exact(PollingRate::Hz125));
        assert_eq!(parse_rate_action("4000").unwrap(), RateAction::Exact(PollingRate::Hz4000));
        assert_eq!(parse_rate_action("up").unwrap(), RateAction::Up);
        assert_eq!(parse_rate_action("DOWN").unwrap(), RateAction::Down);
        assert_eq!(parse_rate_action("cycle").unwrap(), RateAction::Cycle);
        assert_eq!(parse_rate_action("max").unwrap(), RateAction::Max);
        assert!(parse_rate_action("8000").is_err());
        assert!(parse_rate_action("fast").is_err());
    }

    #[test]
    fn cli_wait_defaults_to_five_seconds_and_accepts_override() {
        let default_cli = Cli::try_parse_from(["hypernova-ctl", "rate", "4000"]).unwrap();
        assert_eq!(default_cli.wait, 5);

        let zero_wait = Cli::try_parse_from(["hypernova-ctl", "--wait", "0", "rate", "1000"])
            .unwrap();
        assert_eq!(zero_wait.wait, 0);

        let after_subcmd = Cli::try_parse_from(["hypernova-ctl", "rate", "up", "--wait", "10"])
            .unwrap();
        assert_eq!(after_subcmd.wait, 10);
    }
}
