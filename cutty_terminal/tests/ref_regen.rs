#![cfg(feature = "serde")]

use serde::Deserialize;
use serde_json as json;

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use cutty_terminal::event::{Event, EventListener};
use cutty_terminal::term::test::TermSize;
use cutty_terminal::term::{Config, Term};
use cutty_terminal::vte::ansi;

#[derive(Deserialize, Default)]
struct RefConfig {
    history_size: u32,
}

#[derive(Copy, Clone)]
struct Mock;

impl EventListener for Mock {
    fn send_event(&self, _event: Event) {}
}

#[test]
#[ignore = "developer utility to regenerate ref grids from recordings"]
fn regenerate_ref_grids() {
    for dir in ref_dirs() {
        regenerate_grid(&dir);
    }
}

fn ref_dirs() -> Vec<PathBuf> {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ref"));
    let mut dirs = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    dirs.sort();
    dirs
}

fn regenerate_grid(dir: &Path) {
    let recording = read_u8(dir.join("cutty.recording"));
    let serialized_size = fs::read_to_string(dir.join("size.json")).unwrap();
    let serialized_cfg = fs::read_to_string(dir.join("config.json")).unwrap();

    let size: TermSize = json::from_str(&serialized_size).unwrap();
    let ref_config: RefConfig = json::from_str(&serialized_cfg).unwrap();

    let options =
        Config { scrolling_history: ref_config.history_size as usize, ..Default::default() };

    let mut terminal = Term::new(options, &size, Mock);
    let mut parser: ansi::Processor = ansi::Processor::new();
    parser.advance(&mut terminal, &recording);

    let mut grid = terminal.grid().clone();
    grid.initialize_all();
    grid.truncate();

    let serialized_grid = json::to_string(&grid).unwrap();
    File::create(dir.join("grid.json"))
        .and_then(|mut file| file.write_all(serialized_grid.as_bytes()))
        .unwrap();
}

fn read_u8<P>(path: P) -> Vec<u8>
where
    P: AsRef<Path>,
{
    let mut res = Vec::new();
    File::open(path.as_ref()).unwrap().read_to_end(&mut res).unwrap();
    res
}
