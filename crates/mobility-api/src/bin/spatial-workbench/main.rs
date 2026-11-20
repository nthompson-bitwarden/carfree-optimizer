mod model;
mod server;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let port: u16 = args.next().as_deref().unwrap_or("8047").parse()?;
    if args.next().is_some() {
        return Err("usage: spatial-workbench [loopback-port]".into());
    }
    server::serve(port)
}
