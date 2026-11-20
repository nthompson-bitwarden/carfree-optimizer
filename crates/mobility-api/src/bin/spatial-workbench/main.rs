mod model;
mod server;
mod telemetry;
mod workspace;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let port: u16 = match args.next() {
        Some(flag) if flag == "--version" && args.next().is_none() => {
            println!("spatial-workbench {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(port) => port.to_str().ok_or("Invalid port.")?.parse()?,
        None => 8047,
    };
    let workspace =
        match (args.next(), args.next(), args.next()) {
            (None, None, None) => None,
            (Some(directory), Some(owner_file), None) => Some(workspace::Workspace::open(
                directory.as_ref(),
                owner_file.as_ref(),
            )?),
            _ => return Err(
                "usage: spatial-workbench [loopback-port [workspace-directory owner-token-file]]"
                    .into(),
            ),
        };
    server::serve(port, workspace)
}
