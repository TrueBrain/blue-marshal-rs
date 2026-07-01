use std::env;
use std::fs;
use std::process::ExitCode;

fn usage() -> ! {
    eprintln!("usage:");
    eprintln!("  marshal-tool to-json <in.marshal> <out.json>");
    eprintln!("  marshal-tool from-json <in.json> <out.marshal>");
    std::process::exit(2);
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        usage();
    }
    let result = match args[1].as_str() {
        "to-json" => to_json(&args[2], &args[3]),
        "from-json" => from_json(&args[2], &args[3]),
        _ => usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn to_json(input: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(input)?;
    let decoded = blue_marshal::decode(&bytes)?;
    let json = blue_marshal::to_json(&decoded.value);
    fs::write(output, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

fn from_json(input: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let text = fs::read_to_string(input)?;
    let json: serde_json::Value = serde_json::from_str(&text)?;
    let value = blue_marshal::from_json(&json)?;
    let bytes = blue_marshal::encode(&value, &blue_marshal::EncodeOptions::default())?;
    fs::write(output, bytes)?;
    Ok(())
}
