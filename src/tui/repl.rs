use crate::conversation::ConversationRuntime;
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use crossterm::ExecutableCommand;
use std::io::{self, BufRead, Write};

/// REPL ループを実行。
pub fn run_repl(runtime: &mut ConversationRuntime) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // プロンプト表示
        stdout
            .execute(SetForegroundColor(Color::Cyan))
            .ok();
        print!("> ");
        stdout.execute(ResetColor).ok();
        stdout.flush().ok();

        // 入力読み取り
        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // 終了コマンド
        match input {
            "/exit" | "/quit" | "/q" => break,
            "/clear" => {
                eprintln!("[ALICE] conversation cleared");
                continue;
            }
            "/help" => {
                print_help();
                continue;
            }
            _ => {}
        }

        // ターン実行
        stdout
            .execute(SetForegroundColor(Color::Green))
            .ok();
        match runtime.run_turn(input) {
            Ok(response) => {
                stdout.execute(ResetColor).ok();
                println!("{response}");
            }
            Err(e) => {
                stdout
                    .execute(SetForegroundColor(Color::Red))
                    .ok();
                eprintln!("[error] {e}");
                stdout.execute(ResetColor).ok();
            }
        }
        stdout.execute(ResetColor).ok();
    }
}

fn print_help() {
    eprintln!("Commands:");
    eprintln!("  /help   — show this help");
    eprintln!("  /clear  — clear conversation");
    eprintln!("  /exit   — exit ALICE");
}
