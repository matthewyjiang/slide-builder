use anyhow::{bail, Context, Result};
use slide_builder_pty::{IsolatedWorkspace, Key, PtyHarness, PtySize};
use std::{path::PathBuf, time::Duration};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let scenario = args.next().unwrap_or_else(|| "help".into());
    if scenario == "help" || scenario == "--help" {
        println!("Usage: slide-builder-pty <picker|onboarding|all> <slide-builder binary> [artifact directory]");
        return Ok(());
    }
    if !["picker", "onboarding", "all"].contains(&scenario.as_str()) {
        bail!("unknown scenario: {scenario}");
    }
    let binary = PathBuf::from(
        args.next()
            .context("provide the slide-builder binary path")?,
    )
    .canonicalize()?;
    let artifacts = PathBuf::from(args.next().unwrap_or_else(|| "target/pty-artifacts".into()));
    if args.next().is_some() {
        bail!("unexpected extra arguments");
    }
    for name in ["picker", "onboarding"] {
        if scenario != "all" && scenario != name {
            continue;
        }
        for size in [PtySize::new(32, 110), PtySize::new(24, 60)] {
            let workspace = IsolatedWorkspace::new()?;
            let cwd = workspace.cwd();
            std::fs::create_dir(cwd.join("sample-folder"))?;
            let child_args = if name == "onboarding" {
                vec!["test-deck.pptx"]
            } else {
                vec![]
            };
            let mut harness = PtyHarness::spawn(
                &binary,
                &child_args,
                size,
                &workspace.environment(),
                &cwd,
                &artifacts,
            )?;
            let timeout = Duration::from_secs(10);
            let title = if name == "picker" {
                "Open deck"
            } else {
                "Welcome to slide-builder"
            };
            harness.wait_for_text(title, timeout)?;
            if name == "picker" {
                harness.wait_for_text("sample-folder", timeout)?;
                harness.paste("unmatched")?;
                harness.wait_for_text_gone("sample-folder", timeout)?;
                for _ in 0.."unmatched".len() {
                    harness.key(Key::Backspace)?;
                }
                harness.wait_for_text("sample-folder", timeout)?;
                harness.send(b"sample")?;
                harness.wait_for_text("│sample ", timeout)?;
                harness.resize(PtySize::new(28, 72))?;
                // The picker redraws after its resize event; filter input also exercises redraw.
                harness.key(Key::Backspace)?;
                harness.wait_for_text("│sampl ", timeout)?;
            } else {
                harness.wait_for_text("› Ollama ", timeout)?;
                harness.key(Key::Down)?;
                harness.wait_for_text("› Ollama Cloud", timeout)?;
                harness.key(Key::Up)?;
                harness.wait_for_text("› Ollama ", timeout)?;
                harness.key(Key::Enter)?;
                harness.wait_for_text("Choose how to connect", timeout)?;
                harness.key(Key::Esc)?;
                harness.wait_for_text(title, timeout)?;
            }
            let capture = harness.capture(&format!("{name} {size:?}"))?;
            harness.key(Key::Esc)?;
            // Onboarding cancellation is currently reported as an application error.
            harness.expect_exit(if name == "picker" { 0 } else { 1 }, timeout)?;
            println!(
                "PASS {name} {}x{} {}",
                size.cols,
                size.rows,
                capture.display()
            );
        }
    }
    Ok(())
}
