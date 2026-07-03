use clap::{value_parser, Arg, Command};
pub mod config_models;
mod parser;
pub mod utils;

fn main() {
    let matches = Command::new("whoisthat-parser")
        .version("0.1.0")
        .about("Parses V2ray/Hysteria2 URIs and generates client config")
        .arg(
            Arg::new("uri")
                .help("V2ray/Hysteria2 URI to parse")
                .index(1),
        )
        .arg(
            Arg::new("socksport")
                .long("socksport")
                .help("Optional SOCKS5 proxy port for inbound")
                .value_name("PORT")
                .value_parser(value_parser!(u16)),
        )
        .arg(
            Arg::new("httpport")
                .long("httpport")
                .help("Optional HTTP proxy port for inbound")
                .value_name("PORT")
                .value_parser(value_parser!(u16)),
        )
        .arg(
            Arg::new("get_metadata")
                .long("get-metadata")
                .help("Only print config meta data")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("get_hysteria_yaml")
                .long("get-hysteria-yaml")
                .help("Emit YAML config for the official hysteria2 client (apernet/hysteria2).")
                .action(clap::ArgAction::SetTrue)
                .requires("socksport"),
        )
        .get_matches();

    let uri = match matches.get_one::<String>("uri") {
        Some(uri) => uri.to_owned(),
        None => match dialoguer::Input::new().interact_text() {
            Ok(uri) => uri,
            Err(e) => {
                eprintln!("Error reading URI from terminal: {}", e);
                std::process::exit(1);
            }
        }
    };
    let socksport = matches.get_one::<u16>("socksport").copied();
    let httpport = matches.get_one::<u16>("httpport").copied();
    let get_metadata = matches.get_flag("get_metadata");
    let get_hysteria_yaml = matches.get_flag("get_hysteria_yaml");

    if get_metadata {
        match parser::get_metadata(uri.as_str()) {
            Ok(meta) => print!("{}", meta),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    if get_hysteria_yaml {
        match parser::create_hysteria2_client_yaml(
            uri.as_str(),
            socksport.unwrap_or(3090),
            httpport,
        ) {
            Ok(yaml) => print!("{}", yaml),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    let json_config = match parser::create_json_config(uri.as_str(), socksport, httpport) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    println!("{}", json_config);
}