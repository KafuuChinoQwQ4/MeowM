//! Decode Excel cells without running formulas or retaining the original file.

use crate::{
    ImportError, InputRow, MAX_IMPORT_ROWS, ParsedImport, Provider, csv, find_header,
    parse_provider_rows,
};
use calamine::{Data, Reader, Xls, Xlsx};
use cfb::CompoundFile;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

const MAX_SHEET_ROWS: usize = MAX_IMPORT_ROWS + 100; // Allow titles and summary rows.
const MAX_COLUMNS: usize = 128;
const MAX_SHEETS: usize = 32;

pub(crate) fn parse_excel(provider: Provider, bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    let sheets = if bytes.starts_with(b"PK") {
        read_xlsx(bytes)?
    } else {
        read_xls(bytes)?
    };
    let mut selected = None;
    for (name, cells) in sheets {
        let mut rows: Vec<_> = cells
            .iter()
            .map(|(number, cells)| InputRow {
                number: *number,
                cells: cells.iter().map(cell_text).collect(),
                error: None,
            })
            .filter(|row| row.cells.iter().any(|text| !text.trim().is_empty()))
            .collect();
        let Some(header_index) = find_header(&rows, provider) else {
            continue;
        };
        if selected.is_some() {
            return Err(ImportError::Excel(
                "发现多张账单工作表，请将需要的工作表分别保存后导入".into(),
            ));
        }
        let headers = csv::header_map(&rows[header_index].cells);
        let id_columns: Vec<_> = provider
            .id_headers()
            .iter()
            .chain(crate::normalize::ORIGINAL_ORDER_HEADERS.iter())
            .filter_map(|alias| headers.get(&csv::normalize_header(alias)).copied())
            .collect();
        let amount_columns: Vec<_> = provider
            .amount_headers()
            .iter()
            .filter_map(|alias| headers.get(&csv::normalize_header(alias)).copied())
            .collect();
        for row in rows.iter_mut().skip(header_index + 1) {
            let original = &cells[&row.number];
            if original.iter().any(|cell| matches!(cell, Data::Error(_))) {
                row.error = Some(format!(
                    "第 {} 行含 Excel 错误单元格，请修正后重新导入",
                    row.number
                ));
            } else if amount_columns
                .iter()
                .any(|&column| original.get(column).is_some_and(unsafe_numeric_amount))
            {
                row.error = Some(format!(
                    "第 {} 行金额是超大 Excel 数值，可能已丢失分位精度；请从原始账单重新导出为文本后导入",
                    row.number
                ));
            } else if id_columns
                .iter()
                .any(|&column| original.get(column).is_some_and(unsafe_numeric_id))
            {
                row.error = Some(format!(
                    "第 {} 行交易号为超过 15 位或非整数的数值，可能已丢失精度；请从原账单按文本重新导出交易号",
                    row.number
                ));
            }
        }
        selected = Some((name, rows));
    }
    let (_, rows) = selected.ok_or_else(|| {
        ImportError::Excel("未找到包含日期和金额表头的账单工作表，请检查账单来源和表头".into())
    })?;
    parse_provider_rows(rows, provider.source_kind(), provider)
}

type Sheets = Vec<(String, BTreeMap<usize, Vec<Data>>)>;

fn read_xlsx(bytes: &[u8]) -> Result<Sheets, ImportError> {
    // Bound expansion before loading shared strings, styles or worksheet XML.
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(excel_error)?;
    if archive.len() > 1024 {
        return Err(ImportError::LimitExceeded(
            "Excel 文件内容过多，请精简后导入".into(),
        ));
    }
    let mut expanded_size = 0u64;
    for index in 0..archive.len() {
        expanded_size =
            expanded_size.saturating_add(archive.by_index(index).map_err(excel_error)?.size());
        if expanded_size > 32 * 1024 * 1024 {
            return Err(ImportError::LimitExceeded(
                "Excel 解压后内容不能超过 32 MiB，请拆分文件".into(),
            ));
        }
    }
    let mut workbook = Xlsx::new(Cursor::new(bytes)).map_err(excel_error)?;
    let names = workbook.sheet_names();
    check_sheet_count(names.len())?;
    let mut sheets = Vec::new();
    for name in names {
        let mut reader = workbook
            .worksheet_cells_reader(&name)
            .map_err(excel_error)?;
        // Read sparse cells directly: a distant formatted cell must not allocate
        // a dense matrix with millions of otherwise empty rows or columns.
        let mut rows = BTreeMap::<usize, Vec<Data>>::new();
        while let Some(cell) = reader.next_cell().map_err(excel_error)? {
            let value = Data::from(cell.get_value().clone());
            if value == Data::Empty {
                continue;
            }
            let (row, column) = cell.get_position();
            if column as usize >= MAX_COLUMNS {
                return Err(ImportError::LimitExceeded(
                    "Excel 工作表最多支持 128 列，请删除无关列后导入".into(),
                ));
            }
            let values = rows.entry(row as usize + 1).or_default();
            values.resize(values.len().max(column as usize + 1), Data::Empty);
            values[column as usize] = value;
            check_row_count(rows.len())?;
        }
        sheets.push((name, rows));
    }
    Ok(sheets)
}

fn read_xls(bytes: &[u8]) -> Result<Sheets, ImportError> {
    preflight_xls(bytes)?;
    let mut workbook = Xls::new(Cursor::new(bytes)).map_err(excel_error)?;
    let names = workbook.sheet_names();
    check_sheet_count(names.len())?;
    let mut sheets = Vec::new();
    for name in names {
        let range = workbook.worksheet_range(&name).map_err(excel_error)?;
        if range.width() > MAX_COLUMNS {
            return Err(ImportError::LimitExceeded(
                "Excel 工作表最多支持 128 列，请删除无关列后导入".into(),
            ));
        }
        let (start_row, start_column) = range.start().unwrap_or_default();
        if start_column as usize + range.width() > MAX_COLUMNS {
            return Err(ImportError::LimitExceeded(
                "Excel 工作表最多支持 128 列，请删除无关列后导入".into(),
            ));
        }
        let mut rows = BTreeMap::new();
        for (index, row) in range.rows().enumerate() {
            if row.iter().all(|cell| cell == &Data::Empty) {
                continue;
            }
            let mut cells = vec![Data::Empty; start_column as usize];
            cells.extend_from_slice(row);
            rows.insert(start_row as usize + index + 1, cells);
            check_row_count(rows.len())?;
        }
        sheets.push((name, rows));
    }
    Ok(sheets)
}

/// Calamine materializes XLS sheets as dense ranges. Inspect BIFF extents first
/// so a tiny file with distant cells or a hostile DIMENSIONS record cannot force
/// a huge allocation before the usual row/column checks run.
fn preflight_xls(bytes: &[u8]) -> Result<(), ImportError> {
    let mut compound = CompoundFile::open(Cursor::new(bytes)).map_err(excel_error)?;
    let stream_path = if compound.exists("/Workbook") {
        "/Workbook"
    } else if compound.exists("/Book") {
        "/Book"
    } else {
        return Err(ImportError::Excel("XLS 文件缺少工作簿数据流".into()));
    };
    let mut stream = compound.open_stream(stream_path).map_err(excel_error)?;
    if stream.len() > 5 * 1024 * 1024 {
        return Err(ImportError::LimitExceeded(
            "XLS 工作簿数据不能超过 5 MiB".into(),
        ));
    }
    let mut biff = Vec::with_capacity(stream.len() as usize);
    stream.read_to_end(&mut biff).map_err(excel_error)?;

    let mut sheets = Vec::new();
    let mut cursor = 0;
    let mut found_globals = false;
    while let Some(record) = next_biff_record(&biff, &mut cursor)? {
        let (id, data) = record;
        if id == 0x0085 {
            if data.len() < 4 {
                return Err(ImportError::Excel("XLS 工作表索引无效".into()));
            }
            sheets.push(u32::from_le_bytes(data[..4].try_into().unwrap()) as usize);
            if sheets.len() > MAX_SHEETS {
                return Err(ImportError::LimitExceeded(
                    "Excel 工作表不能超过 32 张，请拆分文件".into(),
                ));
            }
        }
        if id == 0x000A {
            found_globals = true;
            break;
        }
    }
    if !found_globals || sheets.is_empty() {
        return Err(ImportError::Excel(
            "XLS 工作簿缺少有效的 BIFF 工作表".into(),
        ));
    }
    for start in sheets {
        if start >= biff.len() {
            return Err(ImportError::Excel("XLS 工作表偏移超出文件范围".into()));
        }
        let mut cursor = start;
        let mut nested = 0usize;
        let mut seen_bof = false;
        let mut min_row = usize::MAX;
        let mut max_row = 0usize;
        let mut min_col = usize::MAX;
        let mut max_col = 0usize;
        let mut found_eof = false;
        while let Some((id, data)) = next_biff_record(&biff, &mut cursor)? {
            match id {
                0x0809 => {
                    if seen_bof {
                        nested += 1;
                    } else {
                        seen_bof = true;
                    }
                }
                0x000A if nested > 0 => nested -= 1,
                0x000A => {
                    found_eof = true;
                    break;
                }
                0x0200 => {
                    let (first_row, end_row, first_col, end_col) = match data.len() {
                        10 => (
                            read_u16(data, 0) as usize,
                            read_u16(data, 2) as usize,
                            read_u16(data, 4) as usize,
                            read_u16(data, 6) as usize,
                        ),
                        14 | 16 => (
                            read_u32(data, 0) as usize,
                            read_u32(data, 4) as usize,
                            read_u16(data, 8) as usize,
                            read_u16(data, 10) as usize,
                        ),
                        _ => return Err(ImportError::Excel("XLS 工作表尺寸记录无效".into())),
                    };
                    check_xls_dimensions(first_row, end_row, first_col, end_col)?;
                    if end_row > first_row && end_col > first_col {
                        min_row = min_row.min(first_row);
                        max_row = max_row.max(end_row - 1);
                        min_col = min_col.min(first_col);
                        max_col = max_col.max(end_col - 1);
                    }
                }
                0x0006 | 0x0201 | 0x0203 | 0x0204 | 0x0205 | 0x00D6 | 0x027E | 0x00FD => {
                    if data.len() >= 4 {
                        include_xls_cell(
                            read_u16(data, 0) as usize,
                            read_u16(data, 2) as usize,
                            &mut min_row,
                            &mut max_row,
                            &mut min_col,
                            &mut max_col,
                        )?;
                    }
                }
                0x00BD => {
                    if data.len() >= 6 {
                        let row = read_u16(data, 0) as usize;
                        let first_col = read_u16(data, 2) as usize;
                        let end = data.len() - 2;
                        let last_col = read_u16(data, end) as usize;
                        include_xls_cell(
                            row,
                            first_col,
                            &mut min_row,
                            &mut max_row,
                            &mut min_col,
                            &mut max_col,
                        )?;
                        include_xls_cell(
                            row,
                            last_col,
                            &mut min_row,
                            &mut max_row,
                            &mut min_col,
                            &mut max_col,
                        )?;
                    }
                }
                _ => {}
            }
        }
        if !found_eof {
            return Err(ImportError::Excel("XLS 工作表记录不完整".into()));
        }
    }
    Ok(())
}

fn next_biff_record<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
) -> Result<Option<(u16, &'a [u8])>, ImportError> {
    if *cursor == bytes.len() {
        return Ok(None);
    }
    if bytes.len() - *cursor < 4 {
        return Err(ImportError::Excel("XLS BIFF 记录被截断".into()));
    }
    let id = read_u16(bytes, *cursor);
    let length = read_u16(bytes, *cursor + 2) as usize;
    let start = *cursor + 4;
    let end = start
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| ImportError::Excel("XLS BIFF 记录长度无效".into()))?;
    *cursor = end;
    Ok(Some((id, &bytes[start..end])))
}

fn read_u16(bytes: &[u8], start: usize) -> u16 {
    u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], start: usize) -> u32 {
    u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
}

fn check_xls_dimensions(
    first_row: usize,
    end_row: usize,
    first_col: usize,
    end_col: usize,
) -> Result<(), ImportError> {
    if end_row < first_row
        || end_col < first_col
        || first_row >= 65_536
        || end_row > 65_536
        || first_col >= 256
        || end_col > 256
    {
        return Err(ImportError::Excel("XLS 工作表尺寸范围无效".into()));
    }
    if end_col.saturating_sub(first_col) > MAX_COLUMNS {
        return Err(ImportError::LimitExceeded(
            "Excel 工作表最多支持 128 列，请删除无关列后导入".into(),
        ));
    }
    if end_row.saturating_sub(first_row) > MAX_SHEET_ROWS {
        return Err(ImportError::LimitExceeded(
            "每次最多导入 10,000 行；XLS 工作表尺寸过大，请精简文件".into(),
        ));
    }
    Ok(())
}

fn include_xls_cell(
    row: usize,
    col: usize,
    min_row: &mut usize,
    max_row: &mut usize,
    min_col: &mut usize,
    max_col: &mut usize,
) -> Result<(), ImportError> {
    if row >= 65_536 || col >= 256 {
        return Err(ImportError::Excel("XLS 单元格坐标超出格式范围".into()));
    }
    *min_row = (*min_row).min(row);
    *max_row = (*max_row).max(row);
    *min_col = (*min_col).min(col);
    *max_col = (*max_col).max(col);
    if *max_row - *min_row + 1 > MAX_SHEET_ROWS {
        return Err(ImportError::LimitExceeded(
            "每次最多导入 10,000 行；XLS 单元格范围过大，请精简文件".into(),
        ));
    }
    if *max_col - *min_col + 1 > MAX_COLUMNS {
        return Err(ImportError::LimitExceeded(
            "Excel 工作表最多支持 128 列，请删除无关列后导入".into(),
        ));
    }
    Ok(())
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::DateTime(value) if value.is_datetime() => value
            .as_datetime()
            .map(|date| date.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
            .unwrap_or_else(|| cell.to_string()),
        _ => cell.to_string(),
    }
}

fn unsafe_numeric_id(cell: &Data) -> bool {
    match cell {
        Data::Int(value) => value.unsigned_abs() >= 1_000_000_000_000_000,
        Data::Float(value) => !value.is_finite() || value.fract() != 0.0 || value.abs() >= 1e15,
        _ => false,
    }
}

fn unsafe_numeric_amount(cell: &Data) -> bool {
    const MAX_SAFE_NUMERIC_AMOUNT: i64 = 10_000_000_000_000;
    match cell {
        Data::Int(value) => value.unsigned_abs() >= MAX_SAFE_NUMERIC_AMOUNT as u64,
        Data::Float(value) => !value.is_finite() || value.abs() >= MAX_SAFE_NUMERIC_AMOUNT as f64,
        _ => false,
    }
}

fn check_sheet_count(count: usize) -> Result<(), ImportError> {
    if count > MAX_SHEETS {
        return Err(ImportError::LimitExceeded(
            "Excel 工作表不能超过 32 张，请拆分文件".into(),
        ));
    }
    Ok(())
}

fn check_row_count(count: usize) -> Result<(), ImportError> {
    if count > MAX_SHEET_ROWS {
        return Err(ImportError::LimitExceeded(
            "Excel 表头和说明行过多，请精简文件；每次最多导入 10,000 行".into(),
        ));
    }
    Ok(())
}

fn excel_error(_: impl std::fmt::Display) -> ImportError {
    ImportError::Excel("请确认文件是有效且未加密的 .xls 或 .xlsx 账单".into())
}

#[cfg(test)]
mod tests {
    use super::{MAX_SHEET_ROWS, check_xls_dimensions};

    #[test]
    fn xls_dimensions_reject_out_of_format_coordinates_before_calamine_loads_them() {
        assert!(check_xls_dimensions(0, 1000, 65_000, 65_010).is_err());
        assert!(check_xls_dimensions(65_536, 65_537, 0, 4).is_err());
        assert!(check_xls_dimensions(0, 1000, 248, 257).is_err());
        assert!(check_xls_dimensions(0, 1000, 0, 8).is_ok());
        assert!(check_xls_dimensions(0, MAX_SHEET_ROWS, 0, 8).is_ok());
        assert!(check_xls_dimensions(0, MAX_SHEET_ROWS + 1, 0, 8).is_err());
    }
}
