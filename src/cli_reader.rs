use crate::reader::cache::PageCache;
use crate::reader::progress::{ProgressStore, ReadingPosition};
use crate::reader::source::ReaderClient;
use crate::reader::{Chapter, Publication, ReaderSource};
use anyhow::{anyhow, Context, Result};
use std::io::{self, IsTerminal, Write};
use std::path::Path;

const MAX_LISTED: usize = 20;

pub struct MangaSearchOptions {
    pub query: String,
    pub lang: String,
    pub json: bool,
}

pub struct MangaReadOptions {
    pub manga_id: String,
    pub chapter: Option<String>,
    pub lang: String,
    pub from_page: Option<usize>,
    pub export_cbz: Option<String>,
    pub no_browser: bool,
}

/// Run `flix manga search`.
pub async fn search(options: MangaSearchOptions) -> Result<()> {
    if options.query.trim().is_empty() {
        return Err(anyhow!("Search text cannot be empty"));
    }
    println!("Searching MangaDex for \"{}\"...", options.query.trim());
    let client = ReaderClient::new(ReaderSource::MangaDex)?;
    let results = client.search(options.query.trim(), MAX_LISTED).await?;

    if results.is_empty() {
        println!("No manga found for \"{}\".", options.query.trim());
        return Ok(());
    }

    if options.json {
        let json = serde_json::to_string_pretty(&results)?;
        println!("{json}");
        return Ok(());
    }

    print_publications(&results);
    Ok(())
}

/// Run `flix manga read`.
pub async fn read(options: MangaReadOptions, data_dir: &Path) -> Result<()> {
    let client = ReaderClient::new(ReaderSource::MangaDex)?;

    // Fetch chapters for the requested language
    let (chapters, available_languages) = client.chapters(&options.manga_id, &options.lang).await?;

    if chapters.is_empty() {
        if available_languages.is_empty() {
            println!("No chapters found for this manga.");
        } else {
            println!(
                "No chapters in language \"{}\". Available: {}",
                options.lang,
                available_languages.join(", ")
            );
        }
        return Ok(());
    }

    // Select the chapter
    let chapter = select_chapter(&chapters, options.chapter.as_deref())?;
    let Some(chapter) = chapter else {
        return Ok(());
    };

    println!(
        "Fetching pages for chapter {}...",
        chapter.chapter.as_deref().unwrap_or("?")
    );

    // Get page URLs
    let page_set = client.pages(&chapter.id).await?;
    if page_set.page_urls.is_empty() {
        println!("This chapter has no pages.");
        return Ok(());
    }

    let start_page = options.from_page.unwrap_or(1).max(1);
    if start_page > page_set.page_urls.len() {
        return Err(anyhow!(
            "Page {start_page} is beyond the chapter's {} pages",
            page_set.page_urls.len()
        ));
    }

    // Download pages into cache
    let cache = PageCache::new(data_dir);
    let source_name = ReaderSource::MangaDex.to_string();
    let chapter_label = chapter.chapter.as_deref().unwrap_or("unknown");
    let mut cached_paths = Vec::new();

    for (index, url) in page_set.page_urls.iter().enumerate() {
        let page_number = index + 1;
        if page_number < start_page {
            continue;
        }
        let ext = page_extension(url);
        let path = cache.page_path(
            &source_name,
            &options.manga_id,
            chapter_label,
            page_number,
            &ext,
        );
        if !cache.has_page(&path) {
            print!(
                "\rDownloading page {page_number}/{}...",
                page_set.page_urls.len()
            );
            io::stdout().flush()?;
            let data = client
                .download_page(url)
                .await
                .with_context(|| format!("Failed to download page {page_number}"))?;
            cache.store_page(&path, &data).await?;
        }
        cached_paths.push(path);
    }
    if !cached_paths.is_empty() {
        println!();
    }

    // Save reading progress
    let mut progress = ProgressStore::load(data_dir)?;
    progress.set(ReadingPosition {
        publication_id: options.manga_id.clone(),
        chapter_id: chapter.id.clone(),
        page: start_page,
        updated_at: chrono_timestamp(),
    });
    progress.save()?;

    // Export CBZ if requested
    if let Some(ref cbz_path) = options.export_cbz {
        export_cbz(&cached_paths, cbz_path)?;
        println!("CBZ exported to {cbz_path}");
    }

    if options.no_browser {
        println!("Cached pages:");
        for path in &cached_paths {
            println!("  {}", path.display());
        }
    } else {
        // TODO(phase-7): start the local reader server and open the browser.
        // For now, print the cached paths.
        println!("Cached pages (reader server not yet available):");
        for path in &cached_paths {
            println!("  {}", path.display());
        }
    }

    Ok(())
}

fn print_publications(publications: &[Publication]) {
    println!("Search results ({} shown):", publications.len());
    for (position, publication) in publications.iter().enumerate() {
        let year = publication
            .year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "year unknown".to_string());
        let status = publication.status.as_deref().unwrap_or("unknown");
        println!(
            "  {:>2}. {} ({}, {})",
            position + 1,
            publication.title,
            year,
            status
        );
        if let Some(ref desc) = publication.description {
            let short = if desc.len() > 100 {
                format!("{}...", &desc[..100])
            } else {
                desc.clone()
            };
            println!("      {short}");
        }
    }
    println!();
    println!("To read: flix manga read <id>");
    if let Some(first) = publications.first() {
        println!("  e.g.: flix manga read {}", first.id);
    }
}

fn select_chapter<'a>(
    chapters: &'a [Chapter],
    requested: Option<&str>,
) -> Result<Option<&'a Chapter>> {
    if let Some(chapter_num) = requested {
        // Find by chapter number
        let found = chapters
            .iter()
            .find(|c| c.chapter.as_deref() == Some(chapter_num));
        match found {
            Some(ch) => Ok(Some(ch)),
            None => {
                let available: Vec<&str> = chapters
                    .iter()
                    .filter_map(|c| c.chapter.as_deref())
                    .collect();
                eprintln!(
                    "Chapter {chapter_num} not found. Available chapters: {}",
                    available.join(", ")
                );
                Ok(None)
            }
        }
    } else if chapters.len() == 1 {
        Ok(Some(&chapters[0]))
    } else {
        // Interactive selection
        println!("Chapters ({} available):", chapters.len());
        let count = chapters.len().min(MAX_LISTED);
        for (position, chapter) in chapters.iter().take(count).enumerate() {
            let vol = chapter
                .volume
                .as_deref()
                .map(|v| format!("Vol {v} "))
                .unwrap_or_default();
            let ch = chapter.chapter.as_deref().unwrap_or("?");
            let title = chapter.title.as_deref().unwrap_or("");
            let pages = if chapter.pages > 0 {
                format!(" ({} pages)", chapter.pages)
            } else {
                String::new()
            };
            println!("  {:>2}. {vol}Ch {ch} {title}{pages}", position + 1,);
        }
        if !io::stdin().is_terminal() {
            return Ok(Some(&chapters[0]));
        }
        loop {
            let input = read_line("Select a chapter [1], [q] quit: ")?;
            let input = input.trim().to_ascii_lowercase();
            if input.is_empty() {
                return Ok(Some(&chapters[0]));
            }
            if input == "q" || input == "quit" {
                return Ok(None);
            }
            if let Ok(value) = input.parse::<usize>() {
                if (1..=count).contains(&value) {
                    return Ok(Some(&chapters[value - 1]));
                }
            }
            eprintln!("Enter a number from 1 to {count}, or q to quit.");
        }
    }
}

fn export_cbz(page_paths: &[std::path::PathBuf], output: &str) -> Result<()> {
    let file = std::fs::File::create(output).context("Failed to create CBZ file")?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (index, path) in page_paths.iter().enumerate() {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
        let name = format!("{:04}.{ext}", index + 1);
        zip.start_file(&name, options)
            .context("Failed to add file to CBZ")?;
        let data = std::fs::read(path).context("Failed to read cached page")?;
        std::io::Write::write_all(&mut zip, &data).context("Failed to write to CBZ")?;
    }
    zip.finish().context("Failed to finalize CBZ")?;
    Ok(())
}

fn page_extension(url: &str) -> String {
    url.rsplit('.')
        .next()
        .filter(|ext| ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("jpg")
        .to_string()
}

fn chrono_timestamp() -> String {
    // Simple ISO-ish timestamp without pulling in chrono
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

fn read_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}
