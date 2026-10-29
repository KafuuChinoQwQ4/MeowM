//! Decode text and its position in memory; do not render or execute PDF content.

use crate::ImportError;
use pdf_extract::{Document, LoadOptions, MediaBox, OutputDev, OutputError, Transform};

pub(crate) struct Word {
    pub x: f64,
    pub y: f64,
    pub right: f64,
    pub text: String,
}

pub(crate) struct Page {
    pub number: u32,
    pub height: f64,
    pub words: Vec<Word>,
}

#[derive(Default)]
struct TextOutput {
    pages: Vec<Page>,
    word: Option<Word>,
    characters: usize,
}

impl TextOutput {
    fn flush(&mut self) {
        if let Some(word) = self.word.take().filter(|word| !word.text.trim().is_empty()) {
            if let Some(page) = self.pages.last_mut() {
                page.words.push(word);
            }
        }
    }
}

impl OutputDev for TextOutput {
    fn begin_page(
        &mut self,
        number: u32,
        media: &MediaBox,
        _: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.pages.push(Page {
            number,
            height: media.ury - media.lly,
            words: Vec::new(),
        });
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), OutputError> {
        self.flush();
        Ok(())
    }

    fn output_character(
        &mut self,
        transform: &Transform,
        width: f64,
        _: f64,
        font_size: f64,
        text: &str,
    ) -> Result<(), OutputError> {
        self.characters += text.len();
        if self.characters > 4 * 1024 * 1024 {
            return Err(OutputError::IoError(std::io::Error::other(
                "PDF text limit",
            )));
        }
        let page = self
            .pages
            .last()
            .ok_or_else(|| OutputError::IoError(std::io::Error::other("PDF page missing")))?;
        let x = transform.m31;
        let y = page.height - transform.m32;
        let right = x + width * font_size * transform.m11.abs();
        if !x.is_finite() || !y.is_finite() || !right.is_finite() {
            return Err(OutputError::IoError(std::io::Error::other(
                "PDF position invalid",
            )));
        }
        if self
            .word
            .as_ref()
            .is_some_and(|word| (word.y - y).abs() > 1.5)
        {
            self.flush();
        }
        let word = self.word.get_or_insert_with(|| Word {
            x,
            y,
            right,
            text: String::new(),
        });
        word.right = word.right.max(right);
        word.text.push_str(text);
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), OutputError> {
        self.flush();
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), OutputError> {
        self.flush();
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), OutputError> {
        self.flush();
        Ok(())
    }
}

pub(crate) fn read(bytes: &[u8], password: Option<&str>) -> Result<Vec<Page>, ImportError> {
    // Supplying the password at load time is required for encrypted stream lengths.
    // Loading without it and then decrypting can silently lose entire pages.
    let document = Document::load_mem_with_options(
        bytes,
        LoadOptions::with_password(password.unwrap_or_default()),
    )
    .map_err(|error| match error {
        pdf_extract::Error::InvalidPassword => password_error(password),
        pdf_extract::Error::Decryption(
            pdf_extract::encryption::DecryptionError::IncorrectPassword,
        ) => password_error(password),
        _ => ImportError::Pdf("文件损坏或加密格式暂不支持，请检查原始 PDF".into()),
    })?;
    if document.is_encrypted() {
        return Err(password_error(password));
    }
    let count = document.get_pages().len();
    if count == 0 || count > 800 {
        return Err(ImportError::Pdf(
            "PDF 无有效页面或超过 800 页，请拆分后导入".into(),
        ));
    }
    let mut output = TextOutput::default();
    pdf_extract::output_doc(&document, &mut output)
        .map_err(|_| ImportError::Pdf("文字无法完整读取或内容过多，请检查 PDF".into()))?;
    Ok(output.pages)
}

fn password_error(password: Option<&str>) -> ImportError {
    if password.is_some_and(|value| !value.is_empty()) {
        ImportError::PdfIncorrectPassword
    } else {
        ImportError::PdfPasswordRequired
    }
}
