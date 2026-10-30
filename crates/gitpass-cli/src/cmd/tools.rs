use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use zeroize::Zeroizing;
use anyhow::Context;
use anyhow::bail;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Gen { command, no_history, len, words, pin, separator, capitalize, no_symbols, no_digits, copy, clear_secs } => {
            if let Some(cmd) = command {
                match cmd {
                    GenSubcommand::History { action, json, reveal } => {
                        let s = unlock(cli, root)?;
                        match action {
                            Some(GenHistoryAction::Clear) => {
                                s.clear_generator_history()?;
                                eprintln!("cleared generator history");
                            }
                            None => {
                                let history = s.read_generator_history()?;
                                if *json {
                                    let dtos: Vec<serde_json::Value> = history.entries.iter().map(|e| {
                                        serde_json::json!({
                                            "id": e.id,
                                            "value": if *reveal { e.value.as_str() } else { "••••••••" },
                                            "created_at": e.created_at,
                                            "source": e.source,
                                            "host": e.host,
                                        })
                                    }).collect();
                                    println!("{}", serde_json::to_string_pretty(&dtos)?);
                                } else if history.entries.is_empty() {
                                    println!("(no generated passwords in history)");
                                } else {
                                    for entry in &history.entries {
                                        let ts = gitpass_core::device::iso8601(entry.created_at as i64);
                                        let val = if *reveal { entry.value.as_str() } else { "••••••••" };
                                        let host_part = entry.host.as_deref().map(|h| format!("  ({h})")).unwrap_or_default();
                                        println!("{ts}  [{}]  {val}{host_part}", entry.source);
                                    }
                                }
                            }
                        }
                        return Ok(());
                    }
                }
            }
            let args = gitpass_cli::gen::Args { len: *len, words: *words, pin: *pin, separator: *separator,
                capitalize: *capitalize, no_symbols: *no_symbols, no_digits: *no_digits, copy: *copy, clear_secs: *clear_secs };
            let mut out = std::io::stdout();
            let pw = gitpass_cli::gen::generate_pw(&args)?;
            if !*no_history && root.join("format.json").exists() {
                if let Ok(s) = unlock(cli, root) {
                    let _ = s.record_generated_password(&pw, "cli", None);
                }
            }
            gitpass_cli::gen::output_pw(&args, &pw, gitpass_cli::clip::system, &mut out, std::thread::sleep)?;
        }
        Cmd::Run { env_file, no_masking, cmd } => {
            use gitpass_cli::inject;

            if env_file.is_empty() && !std::env::vars().any(|(_, v)| v.contains("gp://")) {
                bail!("no gp:// references found: no --env-file was given and no inherited environment variables contain gp:// references (usage: gitpass run [--env-file F] -- <cmd>)");
            }

            let mut file_texts = Vec::with_capacity(env_file.len());
            for f in env_file {
                let text = Zeroizing::new(std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?);
                file_texts.push(text);
            }
            let file_slices: Vec<&str> = file_texts.iter().map(|s| s.as_str()).collect();

            let s = unlock(cli, root)?;
            let sr = SessionResolver(&s);
            let mut res = inject::Resolved::new(&sr);
            let env = inject::resolve_run_env(std::env::vars(), &file_slices, &mut res)?;
            drop(file_texts);
            let secrets = std::mem::take(&mut res.secrets);
            drop(s);
            let code = inject::run_child(cmd, &env, &secrets, !*no_masking)?;
            drop(env); drop(secrets);
            std::process::exit(code);
        }
        Cmd::Inject { input, output } => {
            use gitpass_cli::inject;
            use std::io::Read;
            let text = Zeroizing::new(match input {
                Some(p) => std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?,
                None => { let mut b = String::new(); std::io::stdin().read_to_string(&mut b)?; b }
            });
            let s = unlock(cli, root)?;
            let sr = SessionResolver(&s);
            let out = inject::render_template(&text, &mut inject::Resolved::new(&sr))?;
            match output {
                Some(p) => {
                    gitpass_core::fsutil::atomic_write(p, out.as_bytes())?;
                    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))?; }
                    eprintln!("wrote {} (mode 0600; contains plaintext secrets)", p.display());
                }
                None => { use std::io::Write; std::io::stdout().write_all(out.as_bytes())?; }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
