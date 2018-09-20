#![allow(unused_variables)]
extern crate actix;
extern crate actix_web;
extern crate bs;
extern crate bytes;
extern crate clap;
extern crate env_logger;
extern crate futures;
extern crate http;
extern crate mime;
extern crate openssl;
extern crate regex;
extern crate serde_yaml;
extern crate url;

use actix_web::{server, App};
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};

use bs::config::get_program_config_from_cli;
use bs::config::get_program_config_from_string;
use bs::config::ProgramStartError;
use bs::fns::proxy_transform;
use bs::options::ProxyOpts;
use bs::options::ProxyScheme;
use bs::preset::AppState;
use bs::preset::Preset;
use bs::preset_m2::M2Preset;
use bs::preset_m2_opts::M2PresetOptions;
use openssl::ssl::SslAcceptorBuilder;
use std::collections::HashMap;
use std::sync::Mutex;

fn main() {
    match get_program_config_from_cli().and_then(run_with_opts) {
        Ok(opts) => println!("Runnin!"),
        Err(e) => eprintln!("{}", e),
    }
}

fn run_with_opts(opts: ProxyOpts) -> Result<(), ProgramStartError> {
    //
    // Logging config
    //
    ::std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    //
    // The underlying Actor System
    //
    let sys = actix::System::new("https-proxy");

    //
    // Enable SSL (self signed
    //
    let ssl_builder = get_ssl_builder();

    //
    // The address that the server will be accessible on
    //
    let local_addr = format!("127.0.0.1:{}", opts.port);

    //
    // Just some fake yaml configuration
    // for now until config can be read from disk
    //
    let config_input = r#"
        presets:
          - name: m2
            options:
              require_path: /js/require.js
              bundle_config: file:test/fixtures/bundle-config.yaml
    "#;

    //
    // Get program configuration, from the input above, and
    // then eventuall from a file
    //
    let program_config = get_program_config_from_string(config_input)
        .map_err(ProgramStartError::ConfigParseError)?;

    let server_opts = opts.clone();

    //
    // Now start the server
    //
    let s = server::new(move || {
        //
        // Use a HashMap + index lookup for anything
        // that implements Preset
        //
        let mut presets_map: HashMap<usize, Box<Preset<AppState>>> = HashMap::new();

        //
        // Loop through any presets and create an instance
        // that's stored in the hashmap based on it's index
        //
        // This is done so that we can use the index later
        // to lookup this item in order
        //
        for (index, p) in program_config.presets.iter().enumerate() {
            match p.name.as_str() {
                "m2" => {
                    let cloned_opts = p.options.clone();
                    let preset_opts: M2PresetOptions = cloned_opts.into();
                    let preset = M2Preset::new(preset_opts);
                    presets_map.insert(index, Box::new(preset));
                }
                _ => println!("unsupported"),
            }
        }

        let mut app_state = AppState {
            opts: opts.clone(),
            rewrites: vec![],
            module_items: Mutex::new(vec![]),
        };

        // Add rewrites phase
        for (index, _) in program_config.presets.iter().enumerate() {
            let subject_preset = presets_map.get(&index).expect("Missing preset");
            app_state.rewrites.extend(subject_preset.rewrites());
        }

        let mut app = App::with_state(app_state);

        // before middlewares
        for (index, _) in program_config.presets.iter().enumerate() {
            let subject_preset = presets_map.get(&index).expect("Missing preset");
            app = subject_preset.add_before_middleware(app);
        }

        // enhances
        for (index, _) in program_config.presets.iter().enumerate() {
            let subject_preset = presets_map.get(&index).expect("Missing preset");
            app = subject_preset.enhance(app);
        }

        let app = app.default_resource(|r| r.f(proxy_transform));

        // finally return the App
        app
    }).workers(1);

    let s = match server_opts.scheme {
        ProxyScheme::Http => s.bind(&local_addr),
        ProxyScheme::Https => s.bind_ssl(&local_addr, ssl_builder),
    };

    s.expect("Couldn't bind").start();

    println!(
        "Started https server: {}://{}",
        server_opts.scheme, local_addr
    );

    let _ = sys.run();

    Ok(())
}

///
/// SSL builder
///
/// Todo: allow key/cert options
///
fn get_ssl_builder() -> SslAcceptorBuilder {
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    builder
        .set_private_key_file("src/key.pem", SslFiletype::PEM)
        .unwrap();
    builder.set_certificate_chain_file("src/cert.pem").unwrap();
    builder
}
