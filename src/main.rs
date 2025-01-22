use std::{
    convert::Infallible, fs::File, io::Write, iter, os::fd::AsRawFd, process::Command,
    thread::sleep, time::Duration,
};

use clap::{arg, Parser};
use cookie::{time::OffsetDateTime, SameSite};
use fantoccini::ClientBuilder;
use nix::sys::socket::{
    accept, bind, connect, getsockname, listen, setsockopt, socket, sockopt, AddressFamily,
    Backlog, SockFlag, SockProtocol, SockType, SockaddrIn,
};

type Result<V> = std::result::Result<V, Box<dyn std::error::Error>>;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "chromedriver")]
    chromedriver_path: String,
    #[arg(long, default_value = "cookies.txt")]
    cookies_txt_path: String,
}

fn ephemeral_port_reserve() -> Result<u16> {
    // Ported from https://github.com/Yelp/ephemeral-port-reserve/blob/master/ephemeral_port_reserve.py
    let s = socket(
        AddressFamily::Inet,
        SockType::Stream,
        SockFlag::empty(),
        SockProtocol::Tcp,
    )?;
    setsockopt(&s, sockopt::ReuseAddr, &true)?;
    bind(s.as_raw_fd(), &SockaddrIn::new(127, 0, 0, 1, 0))?;
    listen(&s, Backlog::new(1)?)?;

    let sockname: SockaddrIn = getsockname(s.as_raw_fd())?;

    let s2 = socket(
        AddressFamily::Inet,
        SockType::Stream,
        SockFlag::empty(),
        SockProtocol::Tcp,
    )?;
    connect(s2.as_raw_fd(), &sockname)?;
    accept(s.as_raw_fd())?;
    Ok(sockname.port())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<Infallible> {
    let args = Args::parse();

    let chromedriver_port = ephemeral_port_reserve()?;
    let _chromedriver_process = Command::new(args.chromedriver_path)
        .arg(format!("--port={}", chromedriver_port))
        .spawn()?;

    let webdriver_client = loop {
        match ClientBuilder::rustls()?
            .capabilities(serde_json::from_str(r#"{"goog:chromeOptions":{"args":["--disable-blink-features=AutomationControlled"]}}"#)?)
            .connect(&format!("http://127.0.0.1:{}", chromedriver_port)).await {
            Err(e   ) => {
                eprintln!("Error starting webdriver client: {}. Retrying in 5 seconds...", e);
                sleep(Duration::from_secs(5));
            },
            Ok(client) => {
                break client;
            },
        }
    };

    webdriver_client.goto("https://www.youtube.com/").await?;
    // Give some time for the sign in button to appear
    sleep(Duration::from_secs(10));

    while let Ok(link) = webdriver_client
        .find(fantoccini::Locator::LinkText("Sign in"))
        .await
    {
        link.click().await?;
        loop {
            if webdriver_client.current_url().await?.domain() != Some("www.youtube.com") {
                println!("Waiting for login flow. Retrying in 5 seconds...");
                sleep(Duration::from_secs(5));
            } else {
                break;
            }
        }
    }

    loop {
        webdriver_client.goto("https://www.youtube.com/").await?;
        let current_url = webdriver_client.current_url().await?;
        let cookies = webdriver_client
            .get_all_cookies()
            .await?
            .into_iter()
            .map(|cookie| {
                [
                    cookie
                        .domain()
                        .unwrap_or(current_url.domain().unwrap_or("")),
                    if cookie.same_site().unwrap_or(SameSite::Strict).is_lax() {
                        "TRUE"
                    } else {
                        "FALSE"
                    },
                    cookie.path().unwrap_or(current_url.path()),
                    if cookie.secure().unwrap_or(true) {
                        "TRUE"
                    } else {
                        "FALSE"
                    },
                    &cookie
                        .expires_datetime()
                        .unwrap_or(OffsetDateTime::now_utc())
                        .unix_timestamp()
                        .to_string(),
                    cookie.name(),
                    cookie.value(),
                ]
                .join("\t")
            })
            .chain(iter::once("".to_string()))
            .collect::<Vec<_>>()
            .join("\n");
        let mut cookies_txt = File::create(&args.cookies_txt_path)?;
        cookies_txt.write_all(cookies.as_bytes())?;
        println!("Wrote {}", args.cookies_txt_path);
        sleep(Duration::from_secs(60 * 60 * 6));
    }
}
