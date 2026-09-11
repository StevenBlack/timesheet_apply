use anyhow::Result;
use clap::Parser;
use chrono::{NaiveDate, Duration};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::path::PathBuf;
use umya_spreadsheet::Workbook;
use umya_spreadsheet::reader::xlsx as reader;
use umya_spreadsheet::writer::xlsx as writer;

#[derive(Parser, Debug)]
#[command(name = "timesheet_apply")]
#[command(about = "Apply row data to charges.xlsx spreadsheet")]
struct Args {
    /// Path to the xlsx file
    #[arg(short = 'x', long, default_value = "./charges.xlsx")]
    xlsx: PathBuf,

    /// Path to the rows file
    #[arg(short = 'r', long, default_value = "./rows.txt")]
    rows: PathBuf,

    /// Path to the unprocessed file
    #[arg(short = 'u', long, default_value = "./unprocessed.txt")]
    unprocessed: PathBuf,
}

fn excel_serial_to_date(serial: f64) -> String {
    // Excel epoch is December 30, 1899
    let excel_epoch = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
    let date = excel_epoch + Duration::days(serial as i64);
    date.format("%Y-%m-%d").to_string()
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Clear unprocessed file at the start
    let mut unprocessed_file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&args.unprocessed)?;

    // Load the workbook
    let mut book: Workbook = reader::read(&args.xlsx)
        .map_err(|e| anyhow::anyhow!("Failed to read xlsx: {}", e))?;
    let sheet = book.sheet_mut(0)?;

    // Read and process rows from the input file
    let rows_file = File::open(&args.rows)?;
    let buf_reader = BufReader::new(rows_file);

    for line in buf_reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse the record: date + spaces + description
        let parts: Vec<&str> = line.splitn(2, |c: char| c.is_whitespace()).collect();
        if parts.len() != 2 {
            writeln!(unprocessed_file, "{}", line)?;
            continue;
        }

        let date_str = parts[0];
        let description = parts[1].trim();

        // Find matching row in xlsx
        let mut found = false;

        // Iterate through all rows starting after preamble (rows 1-5)
        for row_idx in 6..=10000u32 {
            let coord_a = format!("A{}", row_idx);
            let date_cell = sheet.value(coord_a.as_str());
            
            if date_cell.is_empty() {
                break; // Assume end of data when we hit empty cells
            }

            // Convert Excel serial to YYYY-MM-DD format
            let xlsx_date = if let Ok(serial) = date_cell.parse::<f64>() {
                excel_serial_to_date(serial)
            } else {
                date_cell.clone()
            };

            if xlsx_date == date_str {
                // Check if column B is empty
                let coord_b = format!("B{}", row_idx);
                let item_val = sheet.value(coord_b.as_str());
                
                if !item_val.is_empty() {
                    continue; // Item already filled, skip this row
                }

                // Check if rate (column E) > 130.00
                let coord_e = format!("E{}", row_idx);
                if let Some(rate) = sheet.value_number(coord_e.as_str()) {
                    if rate > 130.0 {
                        // Found matching row - write description to column B
                        sheet.cell_mut(coord_b.as_str()).set_value(description);
                        found = true;
                        break;
                    }
                }
            }
        }

        // If no matching row found, add to unprocessed
        if !found {
            writeln!(unprocessed_file, "{}", line)?;
        }
    }

    // Save the workbook
    writer::write(&book, &args.xlsx)
        .map_err(|e| anyhow::anyhow!("Failed to write xlsx: {}", e))?;

    Ok(())
}


