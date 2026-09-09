use anyhow::{anyhow, bail, Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt};

const HEADER_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedTracks {
    pub audio: usize,
    pub subtitle: Option<usize>,
}

#[derive(Default)]
struct Track {
    kind: u64,
    language: String,
    name: String,
    original: bool,
    forced: bool,
    commentary: bool,
    disabled: bool,
}

pub async fn inspect(
    mut reader: impl AsyncRead + Unpin,
    external_english: bool,
) -> Result<SelectedTracks> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 16384];
    while bytes.len() < HEADER_LIMIT {
        let count = reader.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(tracks) = parse_header(&bytes)? {
            return select_tracks(&tracks, external_english);
        }
    }
    bail!("Anime language check failed. Select an MKV source with labeled audio and English subtitles.")
}

fn select_tracks(tracks: &[Track], external_english: bool) -> Result<SelectedTracks> {
    let audio: Vec<_> = tracks.iter().filter(|track| track.kind == 2).collect();
    let usable =
        |track: &&Track| !track.disabled && !track.commentary && !track.name.contains("commentary");
    let original = audio
        .iter()
        .position(|track| usable(track) && track.original);
    let audio = original
        .or_else(|| {
            audio.iter().position(|track| {
                usable(track)
                    && matches!(primary_language(&track.language), "ja" | "jpn" | "japanese")
            })
        })
        .context("Anime original audio is unavailable. Select another source.")?;
    let subtitle = tracks
        .iter()
        .filter(|track| track.kind == 17)
        .position(|track| {
            !track.disabled
                && !track.forced
                && !track.commentary
                && matches!(primary_language(&track.language), "en" | "eng" | "english")
                && !track.name.contains("sign")
                && !track.name.contains("song")
        });
    if subtitle.is_none() && !external_english {
        bail!("Anime English subtitles are unavailable. Select another source.");
    }
    Ok(SelectedTracks { audio, subtitle })
}

fn primary_language(language: &str) -> &str {
    language.split('-').next().unwrap_or_default()
}

fn parse_header(bytes: &[u8]) -> Result<Option<Vec<Track>>> {
    if bytes.len() < 4 {
        return Ok(None);
    }
    if bytes[..4] != [0x1a, 0x45, 0xdf, 0xa3] {
        bail!("Anime language check requires an MKV source.");
    }
    let mut offset = 0;
    while let Some((id, size, header)) = element_header(&bytes[offset..])? {
        offset += header;
        if id == 0x18538067 {
            continue;
        }
        let size = size.context("Anime file has an invalid element size")?;
        let end = offset
            .checked_add(size)
            .context("Anime file element is too large")?;
        if end > bytes.len() {
            return Ok(None);
        }
        if id == 0x1654ae6b {
            return parse_track_list(&bytes[offset..end]).map(Some);
        }
        if id == 0x1f43b675 {
            bail!("Anime track labels are unavailable.");
        }
        offset = end;
    }
    Ok(None)
}

fn parse_track_list(mut bytes: &[u8]) -> Result<Vec<Track>> {
    let mut tracks = Vec::new();
    while !bytes.is_empty() {
        let (id, data, rest) = element(bytes)?;
        if id == 0xae {
            if tracks.len() >= 128 {
                bail!("Anime file has too many tracks.");
            }
            tracks.push(parse_track(data)?);
        }
        bytes = rest;
    }
    Ok(tracks)
}

fn parse_track(mut bytes: &[u8]) -> Result<Track> {
    let mut track = Track::default();
    let mut language_bcp47 = None;
    while !bytes.is_empty() {
        let (id, data, rest) = element(bytes)?;
        match id {
            0x83 => track.kind = unsigned(data)?,
            0x22b59c => track.language = text(data)?,
            0x22b59d => language_bcp47 = Some(text(data)?),
            0x536e => track.name = text(data)?,
            0x55ae => track.original = unsigned(data)? == 1,
            0x55aa => track.forced = unsigned(data)? == 1,
            0x55af => track.commentary = unsigned(data)? == 1,
            0xb9 => track.disabled = unsigned(data)? == 0,
            _ => {}
        }
        bytes = rest;
    }
    if let Some(language) = language_bcp47 {
        track.language = language;
    }
    Ok(track)
}

fn element(bytes: &[u8]) -> Result<(u64, &[u8], &[u8])> {
    let (id, size, header) = element_header(bytes)?.context("Anime track header is incomplete")?;
    let size = size.context("Anime track size is unknown")?;
    let end = header
        .checked_add(size)
        .context("Anime track size is invalid")?;
    let data = bytes
        .get(header..end)
        .context("Anime track data is incomplete")?;
    Ok((id, data, &bytes[end..]))
}

fn element_header(bytes: &[u8]) -> Result<Option<(u64, Option<usize>, usize)>> {
    let Some((id, id_len)) = variable_integer(bytes, true)? else {
        return Ok(None);
    };
    let Some((size, size_len)) = variable_integer(&bytes[id_len..], false)? else {
        return Ok(None);
    };
    let unknown = (1_u64 << (7 * size_len)) - 1;
    let size = if size == unknown {
        None
    } else {
        Some(usize::try_from(size)?)
    };
    Ok(Some((id, size, id_len + size_len)))
}

fn variable_integer(bytes: &[u8], identifier: bool) -> Result<Option<(u64, usize)>> {
    let Some(&first) = bytes.first() else {
        return Ok(None);
    };
    let length = first.leading_zeros() as usize + 1;
    if length > if identifier { 4 } else { 8 } {
        bail!("Anime file header is invalid.");
    }
    let Some(data) = bytes.get(..length) else {
        return Ok(None);
    };
    let first = if identifier {
        first
    } else {
        first & u8::MAX.checked_shr(length as u32).unwrap_or(0)
    };
    let value = data[1..].iter().fold(u64::from(first), |value, byte| {
        (value << 8) | u64::from(*byte)
    });
    Ok(Some((value, length)))
}

fn unsigned(bytes: &[u8]) -> Result<u64> {
    if bytes.len() > 8 {
        bail!("Anime track value is invalid.");
    }
    Ok(bytes
        .iter()
        .fold(0, |value, byte| (value << 8) | u64::from(*byte)))
}

fn text(bytes: &[u8]) -> Result<String> {
    if bytes.len() > 1024 {
        return Err(anyhow!("Anime track label is too large."));
    }
    Ok(std::str::from_utf8(bytes)?
        .trim_end_matches('\0')
        .to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(id: &[u8], data: &[u8]) -> Vec<u8> {
        let mut bytes = id.to_vec();
        bytes.extend_from_slice(&(0x4000_u16 | data.len() as u16).to_be_bytes());
        bytes.extend_from_slice(data);
        bytes
    }

    fn fixture_track(kind: u8, language: &str) -> Vec<u8> {
        let mut fields = element(&[0x83], &[kind]);
        fields.extend(element(&[0x22, 0xb5, 0x9c], language.as_bytes()));
        element(&[0xae], &fields)
    }

    #[tokio::test]
    async fn reads_tracks_from_a_complete_matroska_header() {
        let mut bytes = element(&[0x1a, 0x45, 0xdf, 0xa3], &[]);
        bytes.extend([0x18, 0x53, 0x80, 0x67, 0xff]);
        let mut tracks = fixture_track(2, "eng");
        tracks.extend(fixture_track(2, "jpn"));
        tracks.extend(fixture_track(17, "eng"));
        bytes.extend(element(&[0x16, 0x54, 0xae, 0x6b], &tracks));
        assert_eq!(
            inspect(bytes.as_slice(), false).await.unwrap(),
            SelectedTracks {
                audio: 1,
                subtitle: Some(0)
            }
        );
        for end in 0..bytes.len() {
            assert!(inspect(&bytes[..end], false).await.is_err());
        }
    }

    fn track(kind: u64, language: &str, name: &str) -> Track {
        Track {
            kind,
            language: language.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn selects_japanese_audio_and_full_english_subtitles() {
        let tracks = vec![
            track(2, "eng", ""),
            track(2, "jpn", ""),
            track(17, "eng", "signs and songs"),
            track(17, "en-us", "full dialogue"),
        ];
        assert_eq!(
            select_tracks(&tracks, false).unwrap(),
            SelectedTracks {
                audio: 1,
                subtitle: Some(1)
            }
        );
    }

    #[test]
    fn rejects_dubs_unknown_audio_and_missing_english_subtitles() {
        assert!(select_tracks(&[track(2, "eng", "")], true).is_err());
        assert!(select_tracks(&[track(2, "", "")], true).is_err());
        assert!(select_tracks(&[track(2, "jpn", ""), track(17, "spa", "")], false).is_err());
    }

    #[test]
    fn uses_the_original_track_flag_for_other_original_languages() {
        let mut original = track(2, "zho", "");
        original.original = true;
        assert_eq!(
            select_tracks(&[track(2, "jpn", ""), original], true)
                .unwrap()
                .audio,
            1
        );
    }

    #[test]
    fn rejects_invalid_and_incomplete_headers() {
        assert!(parse_header(b"not matroska").is_err());
        assert!(variable_integer(&[0], false).is_err());
        assert_eq!(variable_integer(&[0x40], false).unwrap(), None);
        assert_eq!(variable_integer(&[0x40, 1], false).unwrap(), Some((1, 2)));
        assert!(parse_track_list(&[0xae, 0x84, 0]).is_err());
    }
}
