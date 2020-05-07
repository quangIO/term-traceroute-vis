use reqwest::Client;
use std::io::{self, BufRead};
use std::sync::mpsc::{self, Sender};
use termion::{
    event::Key,
    input::{MouseTerminal, TermRead},
    raw::IntoRawMode,
    screen::AlternateScreen,
};
use tui::{
    backend::TermionBackend,
    layout::{Constraint, Direction, Layout},
    style::Color,
    widgets::{
        canvas::{Canvas, Map, MapResolution},
        Block, Borders,
    },
    Terminal,
};
use std::net::Ipv4Addr;

#[derive(Debug, serde::Deserialize)]
struct IpInfo {
    ip: String,
    latitude: f64,
    longitude: f64,
    org: Option<String>,
    subdivision: Option<String>,
    subdivision2: Option<String>,
    city: Option<String>,
    country: Option<String>,
}

fn draw_ui<T: tui::backend::Backend>(terminal: &mut Terminal<T>, ip_infos: &[IpInfo]) {
    terminal
        .draw(|mut f| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(100)].as_ref())
                .split(f.size());
            let canvas = Canvas::default()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("traceroute-vis"),
                )
                .paint(|ctx| {
                    ctx.draw(&Map {
                        color: Color::White,
                        resolution: MapResolution::High,
                    });
                    for info in ip_infos {
                        ctx.print(info.longitude, info.latitude, "x", Color::Yellow);
                    }
                })
                .x_bounds([-180.0, 180.0])
                .y_bounds([-90.0, 90.0]);
            f.render_widget(canvas, chunks[0]);
        })
        .unwrap();
}

fn is_local_ip(ip: &str) -> bool {
    let ip: Ipv4Addr = match ip.parse() {
        Ok(x) => x,
        Err(_) => {
            Ipv4Addr::from(0)
        }
    };

    let ip = ip.octets().iter().fold(0usize, |acc, &octet| {
        (acc << 8) | octet as usize
    });
    match ip {
        0 |
        167772160..=184549375 |
        3232235520..=3232301055 |
        2130706432..=2147483647 |
        2851995648..=2852061183 |
        2886729728..=2887778303 |
        3758096384..=4026531839 => true,
        _ => false,
    }
}

async fn process(line: String, sender: Sender<IpInfo>, client: Client) {
    let mut s = line.trim().split_whitespace();
    let _id = s.next();
    let _host = s.next();
    let ip = s.next().unwrap();
    if ip == "*" {
        return;
    }
    let ip = &ip[1..ip.len() - 1];
    let ip = if is_local_ip(ip) { "" } else { ip };

    let response = client
        .get(("https://www.iplocate.io/api/lookup/".to_string() + ip).as_str())
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        _ => return,
    };
    let response: IpInfo = match response.json().await {
        Ok(r) => r,
        _ => return,
    };
    sender.send(response).unwrap();
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let client = reqwest::Client::new();
    let stdin = std::io::stdin();

    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    let (tx, rx) = mpsc::channel();

    let handle = tokio::spawn(async move {
        let mut ip_infos = vec![];
        draw_ui(&mut terminal, &ip_infos);
        for ip in rx {
            ip_infos.push(ip);
            draw_ui(&mut terminal, &ip_infos);
        }
        let stdin = termion::get_tty().unwrap();
        for event in stdin.keys() {
            if event.unwrap() == Key::Char('q') {
                return;
            }
        }
    });
    for line in stdin.lock().lines().skip(1) {
        let line = match line {
            Ok(a) => a,
            _ => continue,
        };
        let tx = tx.clone();
        let client = client.clone();
        tokio::spawn(process(line, tx, client));
    }
    drop(tx);
    handle.await?;
    Ok(())
}
