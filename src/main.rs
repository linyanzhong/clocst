mod languages;
mod renderer;
mod scanner;
mod tree;

use clap::Parser;
use crossterm::style::Color;
use crossterm::terminal;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "clocst", about = "Code line counter with dust-style tree view")]
struct Cli {
    /// Directory to scan (default: current directory)
    path: Option<PathBuf>,

    /// Number of languages to highlight with color
    #[arg(long, default_value = "4")]
    highlight_languages: usize,

    /// Maximum directory depth to expand
    #[arg(long)]
    depth: Option<usize>,

    /// Limit the number of top directories/files shown
    #[arg(short = 'n', long = "number-of-lines")]
    number_of_lines: Option<usize>,

    /// Disable .gitignore / .ignore
    #[arg(long)]
    no_ignore: bool,

    /// Colors for top languages, comma-separated (blue green yellow magenta red cyan white)
    #[arg(long, value_delimiter = ',')]
    colors: Vec<String>,
}

fn parse_color(s: &str) -> Color {
    match s.to_lowercase().as_str() {
        "blue" => Color::Blue,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "magenta" => Color::Magenta,
        "red" => Color::Red,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        _ => Color::White,
    }
}

fn main() {
    let cli = Cli::parse();

    let root = cli.path.unwrap_or_else(|| PathBuf::from("."));
    let root = root.canonicalize().unwrap_or(root);

    let colors: Vec<Color> = if cli.colors.is_empty() {
        vec![Color::Blue, Color::Green, Color::Yellow, Color::Magenta]
    } else {
        cli.colors.iter().map(|s| parse_color(s)).collect()
    };

    let entries = scanner::scan(&root, cli.no_ignore);

    if entries.is_empty() {
        eprintln!("No recognized source files found in {}", root.display());
        return;
    }

    let root_node = tree::build_tree(&root, &entries);
    let scheme = tree::compute_color_scheme(&root_node, cli.highlight_languages, &colors);

    let (term_width, term_height) = terminal::size()
        .map(|(w, h)| (w as usize, h as usize))
        .unwrap_or((80, 24));

    renderer::render(
        &root_node,
        &scheme,
        cli.depth,
        cli.number_of_lines,
        term_height,
        term_width,
    );
}
