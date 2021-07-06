use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::NamedTempFile;

fn main() {
    let files: Vec<(NamedTempFile, &'static str)> = FILES
        .iter()
        .map(|(file_name, plot_name)| {
            let tmp = add_cumulative_columns(Path::new(file_name)).unwrap();
            (tmp, *plot_name)
        })
        .collect();

    for (plot_name, column_idx) in PLOTS.iter() {
        println!("{}", plot_name);

        // plot_defs output uses $COLUMN_IDX so replace $PLOTS before $COLUMN_IDX
        let gnuplot = GNUPLOT_TEMPLATE
            .replace("$PLOTS", &plot_defs(&files))
            .replace("$COLUMN_IDX", &column_idx.to_string())
            .replace("$YLABEL", &plot_name.replace("_", " "));

        let process = Command::new("gnuplot")
            .arg("-p")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Unable to spawn gnuplot process");

        process
            .stdin
            .as_ref()
            .unwrap()
            .write_all(gnuplot.as_bytes())
            .expect("Unable to write gnuplot file to gnuplot stdin");

        let output = process.wait_with_output().expect("gnuplot failed");

        std::fs::write(format!("{}.png", plot_name), output.stdout)
            .expect("Unable to write gnuplot output to file");
    }

    std::mem::forget(files);
}

#[derive(Debug)]
enum Error {
    CSV1(csv::Error),
    CSV2(csv::IntoInnerError<csv::Writer<NamedTempFile>>),
    IntParseError(std::num::ParseIntError),
    IO(std::io::Error),
    String(String),
}

impl From<csv::Error> for Error {
    fn from(err: csv::Error) -> Self {
        Error::CSV1(err)
    }
}

impl From<csv::IntoInnerError<csv::Writer<NamedTempFile>>> for Error {
    fn from(err: csv::IntoInnerError<csv::Writer<NamedTempFile>>) -> Self {
        Error::CSV2(err)
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Error::String(err)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(err: std::num::ParseIntError) -> Self {
        Error::IntParseError(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IO(err)
    }
}

// Given a canister perf CSV file path, write to a temporary path with a "total instructions",
// "total accessed host pages", and "total dirtied host pages" columns.
fn add_cumulative_columns(csv_path: &Path) -> Result<NamedTempFile, Error> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(csv_path)?;

    let mut headers = reader.headers()?.to_owned();

    headers.push_field("total instructions");
    headers.push_field("total accessed host pages");
    headers.push_field("total dirtied host pages");

    let mut records: Vec<csv::StringRecord> = vec![];
    for record in reader.into_records() {
        records.push(record?);
    }

    let mut total_instructions: u64 = 0;
    let mut total_accessed_host_pages: u64 = 0;
    let mut total_dirtied_host_pages: u64 = 0;

    for record in &mut records {
        let instructions = record
            .get(INSTRUCTIONS_COL_IDX - 1)
            .ok_or_else(|| "CSV record doesn't have enough columns".to_owned())?
            .parse::<u64>()?;

        total_instructions += instructions;

        record.push_field(&total_instructions.to_string());

        let accessed_host_pages = record
            .get(ACCESSED_HOST_PAGES_COL_IDX - 1)
            .unwrap()
            .parse::<u64>()
            .unwrap();

        total_accessed_host_pages += accessed_host_pages;

        record.push_field(&total_accessed_host_pages.to_string());

        let dirtied_host_pages = record
            .get(DIRTIED_HOST_PAGES_COL_IDX - 1)
            .unwrap()
            .parse::<u64>()
            .unwrap();

        total_dirtied_host_pages += dirtied_host_pages;

        record.push_field(&total_dirtied_host_pages.to_string());
    }

    let tmp_file = NamedTempFile::new()?;
    let mut csv_writer = csv::Writer::from_writer(tmp_file);
    csv_writer.write_record(&headers)?;

    for record in records {
        csv_writer.write_record(&record)?;
    }

    Ok(csv_writer.into_inner()?)
}

const FILES: [(&str, &str); 2] = [
    ("master_copying_gc.csv", "Simple scheduling"),
    ("scheduling.csv", "Smart scheduling"),
];

fn plot_defs(files: &[(NamedTempFile, &'static str)]) -> String {
    files
        .iter()
        .map(|(file, name)| {
            format!(
                r##""{}" using ($0+1):$COLUMN_IDX with linespoints title "{}", "##,
                file.path().to_string_lossy(),
                name,
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const GNUPLOT_TEMPLATE: &str = r###"
set terminal png notransparent rounded giant font "JetBrains Mono" 24 \
  size 1200,960 

set xtics nomirror
set ytics nomirror

set style line 80 lt 0 lc rgb "#808080"

set border 3 back ls 80 

set style line 81 lt 0 lc rgb "#808080" lw 0.5

set grid xtics
set grid ytics
set grid mxtics
set grid mytics

set grid back ls 81

set style line 1 lt 1 lc rgb "#A00000" lw 2 pt 7 ps 1.5
set style line 2 lt 1 lc rgb "#00A000" lw 2 pt 11 ps 1.5
set style line 3 lt 1 lc rgb "#5060D0" lw 2 pt 9 ps 1.5
set style line 4 lt 1 lc rgb "#0000A0" lw 2 pt 8 ps 1.5
set style line 5 lt 1 lc rgb "#D0D000" lw 2 pt 13 ps 1.5
set style line 6 lt 1 lc rgb "#00D0D0" lw 2 pt 12 ps 1.5
set style line 7 lt 1 lc rgb "#B200B2" lw 2 pt 5 ps 1.5

set datafile separator ','

set xlabel "call"
set ylabel "$YLABEL"

set xrange [0:1000]

plot $PLOTS
"###;

/// 1-based index of "instructions" column in drun generated CSVs
const INSTRUCTIONS_COL_IDX: usize = 3;

/// 1-based index of "accessed host pages" column in drun generated CSVs
const ACCESSED_HOST_PAGES_COL_IDX: usize = 4;

/// 1-based index of "dirtied host pages" column in drun generated CSVs
const DIRTIED_HOST_PAGES_COL_IDX: usize = 5;

/// 1-based column indices and names of plots. Note that column indices are for gnuplot, i.e. they
/// start from 1. Make sure to run `add_cumulative_fields` before using this.
const PLOTS: [(&str, usize); 7] = [
    ("instructions", INSTRUCTIONS_COL_IDX),
    ("accessed_host_pages", ACCESSED_HOST_PAGES_COL_IDX),
    ("dirtied_host_pages", DIRTIED_HOST_PAGES_COL_IDX),
    ("total_Wasm_pages_in_use", 6),
    ("total_instructions", 7),
    ("total_accessed_host_pages", 8),
    ("total_dirtied_host_pages", 9),
];
