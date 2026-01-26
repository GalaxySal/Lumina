use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take_while1},
    character::complete::{char, multispace0},
    combinator::{opt, value},
    multi::many0,
    sequence::{delimited, preceded},
    IResult,
};
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use colored::*;

// ==========================================
// Kip Type System (Semantic Intelligence)
// ==========================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Case {
    Nominative,   // [Yalin]
    Accusative,   // [Belirtme]
    Dative,       // [Yonelme]
    Locative,     // [Bulunma]
    Ablative,     // [Ayrilma]
    Instrumental, // [Vasita]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mood {
    Indicative,   // <Haber>
    Imperative,   // <Emir>
    Optative,     // <Istek>
    Conditional,  // <Sart>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Literal { content: String, case: Case },
    Command { verb: String, mood: Mood, args: Vec<Expr> },
}

// ==========================================
// Parser (using nom)
// ==========================================

fn parse_case(input: &str) -> IResult<&str, Case> {
    delimited(
        char('['),
        alt((
            value(Case::Nominative, tag("Yalin")),
            value(Case::Accusative, tag("Belirtme")),
            value(Case::Dative, tag("Yonelme")),
            value(Case::Locative, tag("Bulunma")),
            value(Case::Ablative, tag("Ayrilma")),
            value(Case::Instrumental, tag("Vasita")),
        )),
        char(']'),
    )(input)
}

fn parse_mood(input: &str) -> IResult<&str, Mood> {
    delimited(
        char('<'),
        alt((
            value(Mood::Indicative, tag("Haber")),
            value(Mood::Imperative, tag("Emir")),
            value(Mood::Optative, tag("Istek")),
            value(Mood::Conditional, tag("Sart")),
        )),
        char('>'),
    )(input)
}

fn parse_literal(input: &str) -> IResult<&str, Expr> {
    let (input, content) = delimited(char('"'), is_not("\""), char('"'))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, case) = opt(parse_case)(input)?;
    
    Ok((input, Expr::Literal {
        content: content.to_string(),
        case: case.unwrap_or(Case::Nominative),
    }))
}

fn parse_command(input: &str) -> IResult<&str, Expr> {
    let (input, verb) = take_while1(|c: char| c.is_alphanumeric())(input)?;
    let (input, _) = multispace0(input)?;
    let (input, mood) = opt(parse_mood)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, args) = many0(preceded(multispace0, parse_literal))(input)?;
    
    Ok((input, Expr::Command {
        verb: verb.to_string(),
        mood: mood.unwrap_or(Mood::Imperative),
        args,
    }))
}

fn parse_expr(input: &str) -> IResult<&str, Expr> {
    preceded(multispace0, alt((parse_command, parse_literal)))(input)
}

// ==========================================
// Semantic Interpreter
// ==========================================

fn validate_semantics(cmd: &Expr) -> Result<()> {
    if let Expr::Command { verb, mood: _, args } = cmd {
        match verb.as_str() {
            "yukle" => {
                // 'yukle' expects Accusative
                for arg in args {
                    if let Expr::Literal { case, .. } = arg {
                        if *case != Case::Accusative {
                            return Err(anyhow!(
                                "Semantic Error: 'yukle' (Load) expects [Belirtme] (Accusative) object, found {:?}.", 
                                case
                            ));
                        }
                    }
                }
            },
            "git" => {
                // 'git' expects Dative
                for arg in args {
                    if let Expr::Literal { case, .. } = arg {
                        if *case != Case::Dative {
                            return Err(anyhow!(
                                "Semantic Error: 'git' (Go) expects [Yonelme] (Dative) target, found {:?}.", 
                                case
                            ));
                        }
                    }
                }
            },
            _ => {} // Allow others for now
        }
    }
    Ok(())
}

fn eval(expr: &Expr) -> Result<String> {
    validate_semantics(expr)?;
    match expr {
        Expr::Command { verb, mood, args } => {
            let args_str: Vec<String> = args.iter().map(|a| format!("{:?}", a)).collect();
            Ok(format!("Executing: {} ({:?}) with args: {:?}", verb.green(), mood, args_str))
        },
        Expr::Literal { content, case } => {
            Ok(format!("Literal: {} [{:?}]", content, case))
        }
    }
}

// ==========================================
// Main Entry
// ==========================================

fn main() -> Result<()> {
    println!("{}", "Kip Semantic Intelligence (Rust) v0.3.0".bold().blue());
    println!("Type 'exit' to quit.");

    // Check if piped input exists
    if !atty::is(atty::Stream::Stdin) {
        use std::io::{self, Read};
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        for line in buffer.lines() {
             if line.trim() == "exit" { break; }
             if line.trim().is_empty() { continue; }
             process_input(line);
        }
        return Ok(());
    }

    // Interactive Mode
    let mut rl = DefaultEditor::new()?;
    loop {
        let readline = rl.readline("kip> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line == "exit" {
                    break;
                }
                if line.is_empty() {
                    continue;
                }
                rl.add_history_entry(line)?;
                process_input(line);
            },
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            },
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            },
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}

fn process_input(input: &str) {
    match parse_expr(input) {
        Ok((_, ast)) => {
            match eval(&ast) {
                Ok(result) => println!("{} {}", "=>".green(), result),
                Err(e) => println!("{} {}", "RUNTIME ERROR:".red(), e),
            }
        },
        Err(e) => println!("{} {:?}", "Parse Error:".red(), e),
    }
}
