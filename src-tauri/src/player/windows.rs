use super::{pick_smtc, smtc_elapsed_ms, smtc_source, NowPlaying, Player, PlayerError};
use ::windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct WinSmtcPlayer;

fn err(e: ::windows::core::Error) -> PlayerError {
    PlayerError(format!("SMTC: {e}"))
}

fn read_session(s: &Session) -> Result<NowPlaying, PlayerError> {
    let props = s.TryGetMediaPropertiesAsync().map_err(err)?.get().map_err(err)?;
    let tl = s.GetTimelineProperties().map_err(err)?;
    let status = s.GetPlaybackInfo().map_err(err)?.PlaybackStatus().map_err(err)?;
    let is_playing = status == Status::Playing;
    let duration_ms = (tl.EndTime().map_err(err)?.Duration / 10_000).max(0) as u64;
    let mut position_ms = (tl.Position().map_err(err)?.Duration / 10_000).max(0) as u64;
    if is_playing {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        position_ms += smtc_elapsed_ms(tl.LastUpdatedTime().map_err(err)?.UniversalTime, now);
    }
    if duration_ms > 0 {
        position_ms = position_ms.min(duration_ms);
    }
    Ok(NowPlaying {
        title: props.Title().map_err(err)?.to_string(),
        artist: props.Artist().map_err(err)?.to_string(),
        album: props.AlbumTitle().map_err(err)?.to_string(),
        duration_ms,
        position_ms,
        is_playing,
    })
}

impl Player for WinSmtcPlayer {
    fn now_playing(&self) -> Result<Option<NowPlaying>, PlayerError> {
        let mgr = Manager::RequestAsync().map_err(err)?.get().map_err(err)?;
        let sessions = mgr.GetSessions().map_err(err)?;
        let mut candidates = Vec::new();
        for s in sessions {
            let id = s.SourceAppUserModelId().map_err(err)?.to_string();
            let Some(src) = smtc_source(&id) else { continue };
            // Uma sessão de navegador com erro não pode derrubar a leitura do Spotify.
            match read_session(&s) {
                Ok(np) => candidates.push((src, np)),
                Err(e) => eprintln!("player ({id}): {e}"),
            }
        }
        Ok(pick_smtc(candidates))
    }
}
