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
extern crate url;

use actix_web::{server, App};
use clap::App as ClapApp;
use clap::Arg;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};

use actix_web::http::Method;
use actix_web::middleware::Finished;
use actix_web::middleware::Middleware;
use actix_web::middleware::Started;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use bs::fns::proxy_transform;
use bs::options::{get_host, ProxyOpts};
use bs::preset_m2::ReqCatcher;
use bs::preset_m2::M2Prest;
use url::Url;

fn main() {
    let matches = ClapApp::new("bs-rust")
        .arg(Arg::with_name("input").required(true))
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .takes_value(true),
        ).get_matches();

    match get_host(matches.value_of("input").unwrap_or("")) {
        Ok(host) => {
            let opts = ProxyOpts::new(host)
                .with_port(matches.value_of("port").unwrap_or("8080").parse().unwrap());
            run(opts);
        }
        Err(err) => println!("{}", err),
    }
}

fn run(opts: ProxyOpts) {
    ::std::env::set_var("RUST_LOG", "actix_web=warn");
    env_logger::init();

    let sys = actix::System::new("https-proxy");

    // load ssl keys
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();

    builder
        .set_private_key_file("src/key.pem", SslFiletype::PEM)
        .unwrap();
    builder.set_certificate_chain_file("src/cert.pem").unwrap();

    let local_addr = format!("127.0.0.1:{}", opts.port);

    server::new(move || {
        let res = M2Prest::new();

        // add innitial state & middleware
        let app = App::with_state(opts.clone());
        let app = app.middleware(ReqCatcher::new());

        // add any additional resource methods
        let app = res.resources.into_iter().fold(app, |acc_app, (path, cb)| {
            acc_app.resource(&path, move |r| r.method(Method::GET).f(cb))
        });

        // now add the default response type
        let app = app.default_resource(|r| r.f(proxy_transform));

        // finally return the App
        app
    }).bind_ssl(&local_addr, builder)
    .unwrap()
    .start();

    println!("Started https server: https://{}", local_addr);
    let _ = sys.run();
}
