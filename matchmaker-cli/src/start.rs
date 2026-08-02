use std::{
    collections::HashMap,
    env::set_current_dir,
    io::Read,
    path::Path,
    process::{Command, Stdio, exit},
    sync::{Arc, Mutex},
};

use crate::{
    action::{ActionContext, MMAction, action_handler},
    clap::Cli,
    color::apply_color_spec,
    config::PartialConfig,
    paths::{last_key_path, presets_path},
    register::MMExt,
    utils::{expand_tilde, guess_clip_cmd, guess_editor_cmd, guess_pager_cmd},
};
use crate::{config::Config, paths::default_config_path};
use cba::{
    _wbog,
    bait::{OptionExt, ResultExt, TransformExt},
    bo::{MapReaderError, map_chunks, map_reader_lines, read_to_chunks, write_str},
    bog::BogOkExt,
    ebog, ibog, prints, wbog,
};
use cba::{bo::load_type, broc::CommandExt};
use log::debug;
use matchmaker::{
    Action, ConfigInjector, MatchError, Matchmaker, OddEnds, PickOptions, SSS, acs,
    binds::{BindMap, BindMapExt},
    config::{BlinkRate, CommandSetting, EnvValue, MatcherConfig, StartConfig},
    event::{EventLoop, RenderSender},
    make_previewer,
    message::{Event, Interrupt},
    nucleo::{
        ColumnIndexable,
        injector::{AnsiInjector, Either, IndexedInjector, Injector, SegmentedInjector},
    },
    preview::AppendOnly,
    render::MMState,
    use_formatter,
};
use matchmaker_partial::Apply;

pub fn enter(cli: Cli, partial: PartialConfig) -> anyhow::Result<Config> {
    if cli.test_keys {
        super::crokey::main();
        exit(0);
    }

    let cfg_path = if let Some(p) = &cli.config {
        Path::new(p)
    } else {
        default_config_path()
    };

    if cli.dump_config && atty::is(atty::Stream::Stdout) {
        // if stdout: dump the default cfg with comments
        write_str(cfg_path, crate::config::DEFAULT_CONFIG)?;
        ibog!("Config written to {cfg_path:?}");
        exit(0)
    }

    #[cfg(debug_assertions)]
    if cli.config.is_none() {
        #[cfg(target_os = "windows")]
        write_str(cfg_path, include_str!("../assets/win.dev.toml")).unwrap();

        #[cfg(not(target_os = "windows"))]
        write_str(cfg_path, include_str!("../assets/dev.toml")).unwrap();
    }

    let mut config: Config = if cli.config.is_some() {
        // Explicit --config: load file verbatim, no default layering.
        load_type(cfg_path, |s| toml::from_str(s))._ebog().or_exit()
    } else if cfg_path.exists() {
        // Default user config path: overlay the file on top of the embedded
        // defaults so the user only needs to specify what they want to change
        // (including individual [binds] entries).
        let mut base = Config::default();
        let user: PartialConfig = load_type(cfg_path, |s| toml::from_str(s))._ebog().or_exit();
        base.apply(user);
        base
    } else {
        Config::default()
    };
    // check config
    if config.source.is_some() {
        wbog!("'source' field is not supported in the main config.");
    }

    if config.render.status.template.is_empty() {
        config.render.status.template = r#"\m/\t"#.to_string();
    }

    // apply overrides
    for mut p in cli.r#override {
        if p.is_relative() && p.extension().is_none() {
            let main_p = presets_path().join(&p).join("main.toml");
            p = if !main_p.exists() {
                presets_path().join(p.with_extension("toml"))
            } else {
                main_p
            };
        }
        // no recursion because tail bad
        let o: PartialConfig = load_type(&p, |s| toml::from_str(s))?;

        if let Some(q) = &o.source {
            let source = p.parent().as_ref().unwrap().join(q);
            let o: PartialConfig = load_type(source, |s| toml::from_str(s))?;
            if o.source.is_some() {
                _wbog!("Ignoring 'source' field in nested override.");
            }
            config.apply(o);
        }

        config.apply(o);
        config.envs.insert(
            "MM_OVERRIDE".to_string(),
            EnvValue::new(p.to_string_lossy().to_string()),
        );
    }

    #[cfg(debug_assertions)]
    {
        config.tui.clear_on_exit = false;
    }
    config.apply(partial); // resolve config.exit first

    if !cli.args.is_empty() {
        if !atty::is(atty::Stream::Stdin) && !cli.no_read {
            eprintln!(
                "warning: trailing arguments provided but input is piped. ignoring trailing arguments."
            );
        }
        *COMMAND_ARGS.lock().unwrap() = cli.args;
    }

    // dispatch subcommands
    if cli.last_key {
        let path = config
            .exit
            .last_key_path
            .as_deref()
            .unwrap_or(last_key_path());

        let content = std::fs::read_to_string(path)._elog();
        if let Some(s) = content
            && let s = s.trim()
            && !s.is_empty()
        {
            prints!(s);
            exit(0);
        } else {
            exit(1)
        }
    }

    if cli.fullscreen {
        config.tui.layout = None;
    }

    if cli.sort {
        config.start.sort = true;
    }

    if cli.icons {
        config.render.results.icons = true;
    }

    if cli.symlink_target {
        config.render.results.symlink_target = true;
    }

    if let Some(props) = &cli.media {
        apply_media_props(props, &mut config);
    }

    for spec in &cli.color {
        apply_color_spec(&mut config, spec);
    }

    if let Some(nav) = &cli.nav {
        apply_nav_props(nav, &mut config);
    }

    for nb in &cli.nav_bind {
        if let Some(colon) = nb.find(':') {
            let key = nb[..colon].to_string();
            let action_str = &nb[colon + 1..];
            let parts = split_nav_bind_actions(action_str);

            let mut actions = matchmaker::action::Actions::default();
            let mut parse_ok = true;
            for part in &parts {
                match part.parse::<matchmaker::action::Action<matchmaker::action::NullActionExt>>()
                {
                    Ok(action) => actions.push(action),
                    Err(e) => {
                        eprintln!("warning: invalid --nav-bind action '{}': {}", part, e);
                        parse_ok = false;
                        break;
                    }
                }
            }

            if parse_ok && !actions.is_empty() {
                config.render.ui.nav_binds.insert(key, actions);
            }
        } else {
            eprintln!(
                "warning: --nav-bind '{}' missing ':' separator (expected char:Action)",
                nb
            );
        }
    }

    if config.render.ui.nav_mode && !config.render.ui.nav_basic {
        use matchmaker::action::Actions;
        let mut nb = |k: &str, actions: Actions<matchmaker::action::NullActionExt>| {
            config
                .render
                .ui
                .nav_binds
                .entry(k.to_string())
                .or_insert(actions);
        };
        nb("d", matchmaker::acs![Action::Semantic("fm_delete".into())]);
        nb("a", matchmaker::acs![Action::Semantic("fm_create".into())]);
        nb("r", matchmaker::acs![Action::Semantic("fm_rename".into())]);
        nb("c", matchmaker::acs![Action::Semantic("fm_zip".into())]);
        nb("C", matchmaker::acs![Action::Semantic("fm_unzip".into())]);
        nb(" ", matchmaker::acs![Action::Toggle]);
        nb("y", matchmaker::acs![Action::Semantic("fm_yank".into())]);
        nb("Y", matchmaker::acs![Action::Semantic("fm_unyank".into())]);
        nb("x", matchmaker::acs![Action::Semantic("fm_cut".into())]);
        nb("X", matchmaker::acs![Action::Semantic("fm_uncut".into())]);
        nb("p", matchmaker::acs![Action::Semantic("fm_paste".into())]);
        nb("u", matchmaker::acs![Action::Semantic("fm_undo".into())]);
        nb(
            "ctrl-r",
            matchmaker::acs![Action::Semantic("fm_redo".into())],
        );
        nb("D", matchmaker::acs![Action::Semantic("fm_dragdrop".into())]);
    }

    if cli.dump_config {
        let contents = toml::to_string_pretty(&config).expect("failed to serialize to TOML");

        // if piped: dump the current cfg
        std::io::Write::write_all(&mut std::io::stdout(), contents.as_bytes())?;

        exit(0);
    }

    // check binds
    config.binds = BindMap::default_binds().modify(|x| x.extend(config.binds));
    if config.render.ui.nav_mode {
        config.binds.insert(
            "tab".parse().expect("tab trigger should parse"),
            matchmaker::acs![matchmaker::Action::ToggleFocus],
        );
        config.binds.insert(
            "shift-enter".parse().expect("shift-enter should parse"),
            matchmaker::acs![matchmaker::Action::Print("{=}".to_string()), matchmaker::Action::Quit(2)],
        );
    }
    config.binds.check_cycles().map_err(anyhow::Error::msg)?;
    config.binds.retain(|_, actions| !actions.is_empty());
    config.binds.resolve_semantics();

    for actions in config.binds.values() {
        for a in actions {
            if let Action::Custom(mm) = &a {
                mm.validate()?;
            }
        }
    }

    debug!("Config computed: {config:?}");

    Ok(config)
}

/// Spawns a tokio task mapping f to reader segments.
/// Read aborts on error. Read errors are logged.
pub fn map_reader<E: SSS + std::fmt::Display>(
    reader: impl Read + SSS,
    f: impl FnMut(String) -> Result<(), E> + SSS,
    input_separator: Option<char>,
    abort_empty: Option<RenderSender<MMAction>>,
) -> tokio::task::JoinHandle<Result<usize, MapReaderError<E>>> {
    tokio::task::spawn_blocking(move || {
        let ret = if let Some(delim) = input_separator {
            map_chunks::<E>(read_to_chunks(reader, delim), f, true)
        } else {
            map_reader_lines::<E>(reader, f, true)
        }
        .elog();

        if let Some(render_tx) = abort_empty
            && matches!(ret, Ok(0))
        {
            let _ = render_tx.send(matchmaker::message::RenderCommand::NoMatch);
        }
        log::trace!("All items pushed");
        ret
    })
}

pub static COMMAND_ARGS: Mutex<Vec<std::ffi::OsString>> = Mutex::new(Vec::new());

fn parse_border_type(s: &str) -> ratatui::widgets::BorderType {
    match s.trim().to_ascii_lowercase().as_str() {
        "plain" | "thin" => ratatui::widgets::BorderType::Plain,
        "rounded" => ratatui::widgets::BorderType::Rounded,
        "double" => ratatui::widgets::BorderType::Double,
        _ => ratatui::widgets::BorderType::Thick,
    }
}

fn parse_blink_rate(s: &str) -> BlinkRate {
    match s.trim().to_ascii_lowercase().as_str() {
        "slow" => BlinkRate::Slow,
        "rapid" | "fast" => BlinkRate::Rapid,
        _ => BlinkRate::Normal,
    }
}

fn apply_nav_props(props: &[String], config: &mut Config) {
    config.render.ui.nav_mode = true;
    config.render.ui.nav_bar = None;
    config.render.action.border.sides = Some(ratatui::widgets::Borders::NONE);

    for raw in props {
        for prop in raw.split(',').filter(|s| !s.is_empty()) {
            match prop.split_once(':') {
                None => match prop {
                    "bar" => {
                        config.render.ui.nav_bar = Some(ratatui::widgets::BorderType::Thick);
                    }
                    "action-bar" => {
                        config.render.action.border.sides = Some(ratatui::widgets::Borders::BOTTOM);
                    }
                    "blink" => config.render.ui.nav_blink = true,
                    "bold" => config.render.ui.nav_bold = true,
                    "notify" => config.render.ui.nav_notify = true,
                    "passthrough" => config.render.ui.nav_passthrough = true,
                    "no-filter" => {
                        config.render.query.show = false;
                    }
                    "basic" => {
                        config.render.ui.nav_basic = true;
                        // Silence only the directory-navigation binds (h/l).
                        // Position-jump binds (gg, G, gb, gt) are kept so
                        // basic mode still supports full vertical navigation.
                        let empty = matchmaker::action::Actions::default();
                        for key in &["h", "l"] {
                            config
                                .render
                                .ui
                                .nav_binds
                                .insert(key.to_string(), empty.clone());
                        }
                    }
                    _ => eprintln!("warning: unknown --nav property '{}'", prop),
                },
                Some(("bar", s)) => {
                    config.render.ui.nav_bar = Some(parse_border_type(s));
                }
                Some(("action-bar", s)) => {
                    config.render.action.border.sides = Some(ratatui::widgets::Borders::BOTTOM);
                    config.render.action.border.r#type = Some(parse_border_type(s));
                }
                Some(("blink", s)) => {
                    config.render.ui.nav_blink = true;
                    config.render.ui.nav_blink_rate = parse_blink_rate(s);
                }
                Some(("focus-on-start", s)) => match s.trim().to_ascii_lowercase().as_str() {
                    "picker" => config.render.ui.nav_focus_on_start = matchmaker::config::NavFocus::Picker,
                    _ => config.render.ui.nav_focus_on_start = matchmaker::config::NavFocus::Filter,
                },
                Some(("marker", s)) => config.render.ui.nav_marker = s.to_string(),
                Some(("prompt", s)) => config.render.ui.nav_prompt = s.to_string(),
                Some(("color", s)) => match s.trim().parse::<ratatui::style::Color>() {
                    Ok(color) => config.render.ui.nav_color = color,
                    Err(e) => eprintln!("warning: invalid --nav color '{}': {}", s, e),
                },
                Some((k, _)) => eprintln!("warning: unknown --nav property '{}'", k),
            }
        }
    }
}

fn apply_media_props(props: &[String], config: &mut Config) {
    config.render.preview.media = true;

    for raw in props {
        for prop in raw.split(',').filter(|s| !s.is_empty()) {
            match prop.split_once(':') {
                None => {
                    // Try parsing as a standalone protocol or size
                    match prop.to_ascii_lowercase().as_str() {
                        "kitty" | "sixel" | "halfblocks" | "iterm2" => {
                            config.render.preview.media_protocol = Some(prop.to_string());
                        }
                        "xs" => config.previewer.media_size = 128,
                        "s" => config.previewer.media_size = 256,
                        "m" => config.previewer.media_size = 512,
                        "l" => config.previewer.media_size = 1024,
                        "xl" => config.previewer.media_size = 2048,
                        "full" => config.previewer.media_size = 0,
                        _ => {
                            if let Ok(num) = prop.parse::<u32>() {
                                config.previewer.media_size = num;
                            } else {
                                eprintln!("warning: unknown --media property '{}'", prop);
                            }
                        }
                    }
                }
                Some(("size", s)) => match s.to_ascii_lowercase().as_str() {
                    "xs" => config.previewer.media_size = 128,
                    "s" => config.previewer.media_size = 256,
                    "m" => config.previewer.media_size = 512,
                    "l" => config.previewer.media_size = 1024,
                    "xl" => config.previewer.media_size = 2048,
                    "full" => config.previewer.media_size = 0,
                    _ => {
                        if let Ok(num) = s.parse::<u32>() {
                            config.previewer.media_size = num;
                        } else {
                            eprintln!("warning: invalid --media size value '{}'", s);
                        }
                    }
                },
                Some(("type" | "protocol", s)) => {
                    config.render.preview.media_protocol = Some(s.to_string());
                }
                Some((k, _)) => eprintln!("warning: unknown --media property '{}'", k),
            }
        }
    }
}

/// Split a nav-bind action string on ';' while ignoring semicolons inside
/// parentheses. This allows `Execute(cd {};ls)` to stay a single action.
fn split_nav_bind_actions(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: usize = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }

    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }

    parts
}

pub fn process_envs(mut envs: HashMap<String, EnvValue>) -> HashMap<String, String> {
    let mut processed_envs = HashMap::new();

    // todo: lowpri: should we provision this what is the cost of setting more env vars
    if envs.get("CLIPcmd").is_none() {
        if let Some(v) = std::env::var("CLIPcmd").ok()
            && !v.is_empty()
        {
            envs.insert("CLIPcmd".to_string(), EnvValue::new(v));
        } else {
            if let Some((clip, paste)) = guess_clip_cmd() {
                envs.insert("CLIPcmd".to_string(), EnvValue::new(clip));

                if envs.get("PASTEcmd").is_none()
                    && std::env::var("PASTEcmd")
                        .ok()
                        .map_or(true, |x| x.is_empty())
                {
                    envs.insert("PASTEcmd".to_string(), EnvValue::new(paste));
                }
            }
        }
    }

    if envs.get("PAGER").is_none() && std::env::var("PAGER").ok().map_or(true, |x| x.is_empty()) {
        let ev = EnvValue::new(guess_pager_cmd());
        envs.insert("PAGER".to_string(), ev);
    }

    if envs.get("EDITOR").is_none() && std::env::var("EDITOR").ok().map_or(true, |x| x.is_empty()) {
        let ev = EnvValue::new(guess_editor_cmd());
        envs.insert("PAGER".to_string(), ev);
    }

    // First pass: static envs
    for (k, v) in &envs {
        if !v.value.is_empty() && !v.exec {
            if v.force || std::env::var_os(k).is_none() {
                processed_envs.insert(k.clone(), v.value.to_string());
            }
        }
    }

    // Second pass: dynamic envs
    for (k, v) in &envs {
        if !v.value.is_empty() && v.exec {
            if v.force || std::env::var_os(k).is_none() {
                if let Some(output) = Command::from_script(&v.value)
                    .envs(&processed_envs)
                    .read_to_string()
                    ._elog()
                {
                    processed_envs.insert(k.clone(), output.trim().to_string());
                } else {
                    _wbog!("Failed to execute env command for {}: {}", k, v.value);
                }
            }
        }
    }

    processed_envs
}

pub async fn start(config: Config, no_read: bool, group_prefix: Option<String>) -> Result<(), MatchError> {
    let nav_mode = config.render.ui.nav_mode;
    let nav_notify = config.render.ui.nav_notify;

    let Config {
        render,
        tui,
        previewer,
        matcher: MatcherConfig {
            matcher,
            mut worker,
        },
        columns,
        binds,
        start:
            StartConfig {
                input_separator,
                command: CommandSetting { separator, command },
                directory,
                sync,
                output_separator,
                output_template,
                ansi,
                trim,
                mut additional_commands,
                mode,
                sort,
                reload_interval,
            },
        mut exit,
        mut envs,
        source: _,
    } = config;

    if sort && !worker.sort_threshold.is_smart() {
        // Force nucleo to preserve insertion order (stable sort) so the alphabetically
        // sorted input is displayed in the same order when no query is typed.
        worker.sort_threshold = matchmaker::config::SortThreshold::NEVER;
    }

    // -------- determine command ------------
    if let Some(first) = additional_commands.first_mut() {
        if first.is_empty() {
            *first = command.clone();
        }
    }
    let additional_commands = additional_commands;

    let mut initial_index = 0;
    if additional_commands.len() > 1 {
        if let Ok(index_str) = std::env::var("MM_INDEX") {
            if let Ok(index) = index_str.parse::<usize>() {
                if index < additional_commands.len() {
                    initial_index = index;
                }
            }
        }
    }

    let command = if initial_index > 0 {
        additional_commands[initial_index].clone()
    } else {
        command
    };

    let initial_cmd = (!command.is_empty() && atty::is(atty::Stream::Stdin) || no_read)
        .then_some(command.clone())
        .unwrap_or_default();

    // -------- set envs/directory -----------
    if !additional_commands.is_empty() {
        envs.insert(
            "MM_INDEX".to_string(),
            EnvValue::new(initial_index.to_string()),
        );
    }
    let envs = process_envs(envs);

    if !directory.value.is_empty() {
        let EnvValue { value, force, exec } = directory;

        let mut failed = false;
        if exec {
            if let Some(new_d) = Command::from_script(&value)
                .envs(&envs)
                .read_to_string()
                ._elog()
            {
                let new_d = Path::new(new_d.trim()).to_path_buf();
                if new_d.exists() {
                    failed = set_current_dir(&new_d)
                        .prefix(format!("Failed to switch to {new_d:?}"))
                        ._wbog()
                        .is_some();
                } else {
                    ebog!("Directory does not exist: {}", new_d.display());
                    failed = true;
                }
            } else {
                ebog!("Failed to execute script for directory: {}", value);
                failed = true;
            }
        } else {
            let path = expand_tilde(value.into());
            set_current_dir(&path)
                .prefix(format!("Failed to switch to {path:?}"))
                ._wbog();
        }

        if failed && force {
            std::process::exit(1);
        }
    }

    // ---------------------------------

    let abort_empty = exit.abort_empty;
    let header_lines = render.header.header_lines;
    let print_handle = AppendOnly::new();
    let output_separator = output_separator.clone().unwrap_or("\n".into());
    let preprocess = (ansi, trim);

    if exit.last_key_path.is_none() {
        exit.last_key_path = Some(last_key_path().into())
    }

    let event_loop = EventLoop::with_binds(binds).with_tick_rate(render.tick_rate());

    // set event loop mode
    let mode = if let Some(m) = mode {
        m
    } else {
        match (
            !initial_cmd.is_empty(), // has command => t0
            atty::is(atty::Stream::Stdout),
        ) {
            (true, true) => "tty",
            (true, false) => "t0",
            (false, true) => "piped",
            (false, false) => "t1",
        }
        .to_string()
    };
    log::trace!("mode: {}", mode);
    if let Ok(mut m) = matchmaker::MODE.lock() {
        *m = mode;
    }

    // make matcher and matchmaker with matchmaker-and-matcher-maker
    let copy_trailing_newline = tui.copy_trailing_newline;
    let (
        mut mm,
        injector,
        OddEnds {
            splitter,
            hidden_columns,
            has_error,
        },
    ) = Matchmaker::new_from_config(render, tui, worker, columns, exit, preprocess);

    if has_error {
        return Err(MatchError::Abort(1));
    }
    // make previewer

    if !event_loop.binds.check_traces() {
        // maybe abort with error
    }
    let cli_formatter = Either::Right(
        crate::formatter::format_cli
            as for<'a, 'b, 'c> fn(
                &'a MMState<'b, 'c, matchmaker::ConfigMMItem, matchmaker::ConfigMMInnerItem>,
                &'a str,
                Option<&dyn Fn(String)>,
            ) -> String,
    );
    let binds = event_loop.binds.clone();
    let previewer = make_previewer(
        &mut mm,
        previewer,
        cli_formatter.clone(),
        Box::new(move |config, mode| matchmaker::binds::display_help(&binds, config, Some(mode))),
    );

    // ---------------------- build options ---------------------------

    let bind_tx = event_loop.bind_controller();

    let envs_ = envs.clone();
    let mut options = PickOptions::new()
        .event_loop(event_loop)
        .matcher(matcher.0)
        .previewer(previewer)
        .hidden_columns(hidden_columns)
        .initializer(move |s| {
            s.envs.extend(envs_);
        });

    let render_tx = options.render_tx();
    let push_fn = inject_line(header_lines, render_tx.clone(), injector, group_prefix.clone());

    if let Some(interval) = reload_interval {
        let render_tx = render_tx.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(interval));
            ticker.tick().await; // skip first immediate tick
            loop {
                ticker.tick().await;
                if render_tx.send(matchmaker::message::RenderCommand::Action(matchmaker::action::Action::Reload("".to_string()))).is_err() {
                    break;
                }
            }
        });
    }

    // ---------------------- register handlers ---------------------------
    // print handler (no quoting)
    mm._register_print_handler(
        print_handle.clone(),
        output_separator.clone(),
        cli_formatter.clone(),
    );

    // execute handlers
    mm.register_execute_handler(cli_formatter.clone());
    mm._register_execute_async_handler(cli_formatter.clone());
    mm.register_copy(
        cli_formatter.clone(),
        copy_trailing_newline,
        Some(render_tx.clone()),
    );
    mm._register_become_handler(cli_formatter.clone());
    let chdir_formatter = cli_formatter.clone();
    let mut history: std::collections::HashMap<std::path::PathBuf, String> = std::collections::HashMap::new();
    mm.register_interrupt_handler(Interrupt::ChDir, move |state| {
        let template = state.payload().clone();
        if template.is_empty() {
            return;
        }
        let path = use_formatter(&chdir_formatter, state, &template, None);
        if path.is_empty() {
            return;
        }

        let path_obj = Path::new(&path);
        let mut target_to_select = None;
        if path_obj == Path::new("..") {
            if let Ok(cwd) = std::env::current_dir() {
                if let Some(name) = cwd.file_name() {
                    target_to_select = Some(name.to_string_lossy().to_string());
                }
            }
        }

        let mut old_cwd = None;
        if state.ui.config.nav_mode {
            if let Ok(cwd) = std::env::current_dir() {
                history.insert(cwd.clone(), state.picker_ui.query.input.clone());
                old_cwd = Some(cwd);
            }
        }

        log::debug!("ChDir: {path}");
        if let Err(e) = std::env::set_current_dir(&path) {
            log::warn!("ChDir({path}) failed: {e}");
        } else {
            if let Some(t) = target_to_select {
                unsafe {
                    std::env::set_var("MM_TARGET_ITEM", t);
                }
            } else {
                unsafe {
                    std::env::remove_var("MM_TARGET_ITEM");
                }
            }

            if state.ui.config.nav_mode {
                if let Ok(new_cwd) = std::env::current_dir() {
                    let is_parent = old_cwd.as_ref().map_or(false, |old| old.starts_with(&new_cwd) && old != &new_cwd);
                    if is_parent {
                        if let Some(saved) = history.remove(&new_cwd) {
                            state.picker_ui.query.set(Some(saved), 0);
                        } else {
                            state.picker_ui.query.set(Some(String::new()), 0);
                        }
                    } else {
                        state.picker_ui.query.set(Some(String::new()), 0);
                    }
                }
            }
        }
    });

    let sync_formatter = cli_formatter.clone();
    mm.register_event_handler(Event::Synced, move |state, _| {
        if let Ok(target) = std::env::var("MM_TARGET_ITEM") {
            let count = state.picker_ui.worker.counts().0;
            let mut found = false;
            for i in 0..count {
                state.picker_ui.results.cursor_jump(i);
                let val = use_formatter(&sync_formatter, state, "{=}", None);
                let val_trimmed = val.trim_end_matches('/');
                let target_trimmed = target.trim_end_matches('/');
                if val_trimmed == target_trimmed {
                    found = true;
                    break;
                }
            }
            if !found && count > 0 {
                state.picker_ui.results.cursor_jump(0);
            }
            unsafe {
                std::env::remove_var("MM_TARGET_ITEM");
            }
        }
    });

    // reload handler
    let reload_formatter = cli_formatter.clone();
    let reload_render_tx = render_tx.clone();

    let mut cmd = command.clone();
    mm.register_interrupt_handler(Interrupt::Reload, move |state| {
        if !state.payload().is_empty() {
            cmd = use_formatter(&reload_formatter, state, state.payload(), None);
        };

        if !cmd.is_empty() {
            state.picker_ui.worker.restart(false);
            state.reloading = true;

            let injector = state.injector();
            let injector = IndexedInjector::new_globally_indexed(injector);
            let injector = SegmentedInjector::new(injector, splitter.clone());
            let injector = AnsiInjector::new(injector, preprocess.clone());

            let push_fn = inject_line(
                state.picker_ui.header.config.header_lines,
                reload_render_tx.clone(),
                injector,
                group_prefix.clone(),
            );

            let vars = state.make_env_vars();
            debug!("Reloading: {cmd}");
            state.picker_ui.selector.clear();

            let separator = separator.or(input_separator).unwrap_or('\n');
            let reload_render_tx = reload_render_tx.clone();
            let cmd = cmd.clone();
            tokio::task::spawn_blocking(move || {
                if let Some(out) = Command::from_script(&cmd)
                    .envs(vars)
                    .stdin(Stdio::null())
                    .args(&*COMMAND_ARGS.lock().unwrap())
                    .output()
                    ._elog()
                {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let mut lines: Vec<&str> = text.split(separator).collect();
                    if lines.last() == Some(&"") {
                        lines.pop();
                    }
                    let mut push_fn = push_fn;
                    for line in lines {
                        let _ = push_fn(line.to_string());
                    }
                }
                
                let _ = reload_render_tx.send(matchmaker::message::RenderCommand::Action(
                    matchmaker::action::Action::Custom(crate::action::MMAction::ReloadReady(vec![]))
                ));
            });
        }
    });

    debug!("{mm:?}");

    let mut action_context = ActionContext {
        bind_tx,
        render_tx: render_tx.clone(),
        additional_commands: (additional_commands, initial_index),
        output_template,
        print_handle: print_handle.clone(),
        output_separator: output_separator.clone(),
        clipboard: Arc::new(Mutex::new(None)),
        fm_notify: nav_notify,
        undo_stack: Arc::new(Mutex::new(Vec::new())),
        redo_stack: Arc::new(Mutex::new(Vec::new())),
        fm_action: None,
    };

    options = options
        .ext_handler(move |x, y| action_handler(x, y, &mut action_context))
        .ext_aliaser(|a, _state| match a {
            Action::Accept => acs![MMAction::Accept],
            Action::Semantic(ref s) if s == "fm_create" => acs![MMAction::FmCreateStart],
            Action::Semantic(ref s) if s == "fm_delete" => acs![MMAction::FmDeleteStart],
            Action::Semantic(ref s) if s == "fm_rename" => acs![MMAction::FmRenameStart],
            Action::Semantic(ref s) if s == "fm_unzip" => acs![MMAction::FmUnzipStart],
            Action::Semantic(ref s) if s == "fm_zip" => acs![MMAction::FmZipStart],
            Action::Semantic(ref s) if s == "fm_yank" => acs![MMAction::FmYank],
            Action::Semantic(ref s) if s == "fm_unyank" => acs![MMAction::FmUnyank],
            Action::Semantic(ref s) if s == "fm_cut" => acs![MMAction::FmCut],
            Action::Semantic(ref s) if s == "fm_uncut" => acs![MMAction::FmUncut],
            Action::Semantic(ref s) if s == "fm_paste" => acs![MMAction::FmPaste],
            Action::Semantic(ref s) if s == "fm_undo" => acs![MMAction::FmUndo],
            Action::Semantic(ref s) if s == "fm_redo" => acs![MMAction::FmRedo],
            Action::Semantic(ref s) if s == "fm_dragdrop" => acs![MMAction::FmDragDrop],
            Action::Semantic(ref s) if s == "reloadnext" => acs![MMAction::ReloadNext(None)],
            Action::Semantic(ref s) if s == "reload_local" => acs![MMAction::ReloadNext(Some(0))],
            _ => acs![a],
        });

    if nav_mode {
        log::debug!("Navigation mode enabled");
    }

    // ----------- read -----------------------
    let handle = if sort {
        // Collect all input, sort alphabetically, then inject in sorted order.
        // Alphabetical sort on paths naturally produces tree order because '/' (0x2F)
        // is less than any ASCII letter, so "a/b" always sorts before "a0".
        let sep = separator.or(input_separator).unwrap_or('\n');
        let raw: Vec<u8> = if !atty::is(atty::Stream::Stdin) && !no_read {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf).ok();
            buf
        } else if !command.is_empty() {
            Command::from_script(&command)
                .envs(envs)
                .args(&*COMMAND_ARGS.lock().unwrap())
                .output()
                .map(|o| o.stdout)
                .unwrap_or_default()
        } else {
            eprintln!("error: no input detected.");
            std::process::exit(99)
        };

        let text = String::from_utf8_lossy(&raw);
        let mut lines: Vec<&str> = text.split(sep).collect();
        // Drop a trailing empty token produced by a final newline.
        if lines.last() == Some(&"") {
            lines.pop();
        }
        lines.sort_unstable();
        let sorted = lines.join("\n");
        drop(lines);

        map_reader(
            std::io::Cursor::new(sorted),
            push_fn,
            None, // already newline-separated after join
            abort_empty.then_some(render_tx),
        )
    } else if !atty::is(atty::Stream::Stdin) && !no_read {
        let stdin = std::io::stdin();
        map_reader(
            stdin,
            push_fn,
            input_separator,
            abort_empty.then_some(render_tx),
        )
    } else if !command.is_empty()
        && let Some((mut _child, stdout)) = Command::from_script(&command)
            .envs(envs)
            .args(&*COMMAND_ARGS.lock().unwrap())
            .spawn_piped()
            ._ebog()
    {
        map_reader(
            stdout,
            push_fn,
            separator.or(input_separator),
            abort_empty.then_some(render_tx),
        )
    } else {
        eprintln!("error: no input detected.");
        std::process::exit(99)
    };

    if sync {
        handle.await._wbog(); // warn the mapreader error (?)
    }

    let ret = mm.pick(options).await;

    print_handle.map_to_vec(|s| {
        log::trace!("{s}"); // this apparently helps with a race condition that erases output?
        print!("{}{}", s, output_separator);
    });

    log::trace!("Print complete");

    ret.map(|_| {})
}

use matchmaker::nucleo::{Line, Span};

fn inject_line(
    header_lines: usize,
    render_tx: RenderSender<MMAction>,
    injector: ConfigInjector,
    group_prefix: Option<String>,
) -> impl FnMut(String) -> Result<(), matchmaker::nucleo::WorkerError> + Send {
    let mut header_buf = Vec::with_capacity(header_lines);
    let mut remaining = header_lines;
    let injector = injector;
    let mut current_group: Option<std::sync::Arc<str>> = None;

    // For each row, take the first line of each segmented column, building a Vec<Vec<Line>>
    move |line: String| {
        if let Some(prefix) = &group_prefix {
            if line.starts_with(prefix) {
                current_group = Some(line.strip_prefix(prefix).unwrap().trim().into());
                return Ok(());
            }
        }

        if remaining > 0 {
            let item = injector.wrap((current_group.clone(), line)).unwrap();
            let item = injector.injector.wrap(item).unwrap();
            header_buf.push(item);
            remaining -= 1;

            if remaining == 0 {
                let rows: Vec<Vec<Line>> = header_buf
                    .drain(..)
                    .map(|seg| {
                        let row = (0..seg.len())
                            .map(move |i| {
                                let mut s = seg.get_text(i);
                                if s.lines.is_empty() {
                                    Line::default()
                                } else {
                                    to_static(s.lines.remove(0))
                                }
                            })
                            .collect();
                        trim_trailing_empty(row)
                    })
                    .collect();

                let _ = render_tx.send(matchmaker::message::RenderCommand::HeaderTable(rows));
            }

            Ok(())
        } else {
            injector.push((current_group.clone(), line))
        }
    }
}

fn trim_trailing_empty(mut row: Vec<Line>) -> Vec<Line> {
    while matches!(row.last(), Some(line) if line.iter().all(|x| x.content.is_empty())) {
        row.pop();
    }

    row
}

fn to_static(line: Line<'_>) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| {
                Span::styled(
                    span.content.into_owned(), // force ownership
                    span.style,
                )
            })
            .collect::<Vec<_>>(),
    )
}
