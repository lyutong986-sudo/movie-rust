use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    process::{Child, Command},
    sync::{oneshot, RwLock, Semaphore},
};
use uuid::Uuid;

use crate::{
    config::Config,
    error::AppError,
    models::{EncodingOptionsDto, VideoStreamQuery},
};

/// 杞爜浼氳瘽鐘舵€?
#[derive(Debug, Clone, PartialEq)]
pub enum TranscodingSessionState {
    /// 姝ｅ湪鍒濆鍖?
    Initializing,
    /// 姝ｅ湪杞爜
    Transcoding,
    /// 宸插畬鎴愶紙HLS/DASH 鏂囦欢宸茬敓鎴愶級
    Completed,
    /// 宸插け璐?
    Failed(String),
    /// 宸插彇娑?
    Cancelled,
}

/// 杞爜浼氳瘽淇℃伅
#[derive(Debug, Clone)]
pub struct TranscodingSession {
    /// 浼氳瘽ID
    pub id: Uuid,
    /// 濯掍綋椤笽D
    pub media_item_id: Uuid,
    /// 鐢ㄦ埛ID
    pub user_id: Uuid,
    /// 璁惧ID
    pub device_id: String,
    /// 杞爜鍙傛暟
    pub params: VideoStreamQuery,
    /// 杞爜鍗忚 (HLS/DASH)
    pub protocol: String,
    /// 杈撳嚭鐩綍
    pub output_dir: PathBuf,
    /// 涓绘挱鏀惧垪琛ㄦ枃浠?
    pub playlist_path: PathBuf,
    /// 浼氳瘽鐘舵€?
    pub state: TranscodingSessionState,
    /// 鍒涘缓鏃堕棿
    pub created_at: Instant,
    /// 鏈€鍚庢洿鏂版椂闂?
    pub updated_at: Instant,
    /// 杞爜杩涘害锛?.0-1.0锛?
    pub progress: f32,
    /// 閿欒淇℃伅锛堝鏋滃け璐ワ級
    pub error: Option<String>,
    /// FFmpeg 杩涚▼ID
    pub ffmpeg_pid: Option<u32>,
}

/// FFmpeg杞爜鍣?
#[derive(Clone)]
pub struct Transcoder {
    /// 閰嶇疆
    config: Arc<Config>,
    /// 杞爜浼氳瘽鏄犲皠琛?
    sessions: Arc<RwLock<HashMap<Uuid, TranscodingSession>>>,
    /// 淇″彿閲忛檺鍒舵渶澶ц浆鐮佷細璇濇暟
    session_semaphore: Arc<Semaphore>,
    cancellation_senders: Arc<RwLock<HashMap<Uuid, oneshot::Sender<()>>>>,
}

impl Transcoder {
    /// 鍒涘缓鏂扮殑杞爜鍣?
    pub fn new(config: Arc<Config>) -> Self {
        let max_sessions = config.max_transcode_sessions.max(1);

        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_semaphore: Arc::new(Semaphore::new(max_sessions as usize)),
            cancellation_senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 寮€濮嬭浆鐮佷細璇?
    pub async fn start_transcoding(
        &self,
        media_item_id: Uuid,
        user_id: Uuid,
        device_id: &str,
        params: VideoStreamQuery,
        options: EncodingOptionsDto,
        input_path: &Path,
    ) -> Result<TranscodingSession, AppError> {
        // 妫€鏌ユ槸鍚﹀惎鐢ㄨ浆鐮?
        if !options.enable_transcoding {
            return Err(AppError::TranscodingDisabled);
        }

        // 鑾峰彇淇″彿閲忚鍙紙绛夊緟绌洪棽妲戒綅锛?
        let permit = self
            .session_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // 鐢熸垚浼氳瘽ID鍜岃緭鍑虹洰褰?
        let session_id = Uuid::new_v4();
        let output_dir =
            PathBuf::from(&options.transcoding_temp_path).join(format!("session_{}", session_id));

        // 鍒涘缓杈撳嚭鐩綍
        tokio::fs::create_dir_all(&output_dir)
            .await
            .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        // 纭畾杞爜鍗忚
        let protocol = params
            .transcoding_protocol
            .as_deref()
            .unwrap_or("hls")
            .to_lowercase();

        // 鏋勫缓鎾斁鍒楄〃璺緞
        let playlist_path = output_dir.join("playlist.m3u8");

        // 鍒涘缓鍒濆浼氳瘽瀵硅薄
        let session = TranscodingSession {
            id: session_id,
            media_item_id,
            user_id,
            device_id: device_id.to_string(),
            params: params.clone(),
            protocol: protocol.clone(),
            output_dir: output_dir.clone(),
            playlist_path: playlist_path.clone(),
            state: TranscodingSessionState::Initializing,
            created_at: Instant::now(),
            updated_at: Instant::now(),
            progress: 0.0,
            error: None,
            ffmpeg_pid: None,
        };

        // 瀛樺偍浼氳瘽
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, session.clone());
        }

        tracing::info!(
            "寮€濮嬭浆鐮佷細璇? session_id={}, media_item_id={}, protocol={}, input={:?}",
            session_id,
            media_item_id,
            protocol,
            input_path
        );

        // 鏇存柊浼氳瘽鐘舵€佷负杞爜涓?
        {
            let mut sessions = self.sessions.write().await;
            if let Some(mut session) = sessions.get_mut(&session_id) {
                session.state = TranscodingSessionState::Transcoding;
                session.updated_at = Instant::now();
            }
        }

        // 鏋勫缓FFmpeg鍛戒护琛屽弬鏁?
        let ffmpeg_args =
            self.build_ffmpeg_args(input_path, &output_dir, &params, &protocol, &options)?;
        let ffmpeg_path = if options.encoder_location_type.eq_ignore_ascii_case("Custom") {
            options.encoder_app_path.as_str()
        } else {
            self.config.ffmpeg_path.as_str()
        };

        // 鍚姩FFmpeg杩涚▼
        let mut cmd = Command::new(ffmpeg_path);
        cmd.args(&ffmpeg_args);
        cmd.stdout(std::process::Stdio::null()); // 閲嶅畾鍚戞爣鍑嗚緭鍑?
        cmd.stderr(std::process::Stdio::piped()); // 鎹曡幏鏍囧噯閿欒鐢ㄤ簬璋冭瘯
        cmd.kill_on_drop(true);

        tracing::debug!(
            "鎵цFFmpeg鍛戒护: {} {}",
            ffmpeg_path,
            ffmpeg_args.join(" ")
        );

        let child = cmd.spawn().map_err(|e| {
            tracing::error!("鍚姩FFmpeg杩涚▼澶辫触: {}", e);
            AppError::FfmpegError(format!("鍚姩FFmpeg杩涚▼澶辫触: {}", e))
        })?;

        // 鑾峰彇杩涚▼ID
        let pid = child.id();

        // 鏇存柊浼氳瘽鐘舵€侊紝璁板綍杩涚▼ID
        {
            let mut sessions = self.sessions.write().await;
            if let Some(mut session) = sessions.get_mut(&session_id) {
                session.ffmpeg_pid = pid;
                session.updated_at = Instant::now();
            }
        }

        let (cancel_tx, cancel_rx) = oneshot::channel();
        {
            let mut senders = self.cancellation_senders.write().await;
            senders.insert(session_id, cancel_tx);
        }

        let sessions_clone = self.sessions.clone();
        let senders_clone = self.cancellation_senders.clone();
        let session_id_clone = session_id;

        // 鍚姩鍚庡彴浠诲姟鐩戞帶FFmpeg杩涚▼
        tokio::spawn(async move {
            let _permit = permit;
            let output = wait_for_ffmpeg_or_cancel(child, cancel_rx).await;
            {
                let mut senders = senders_clone.write().await;
                senders.remove(&session_id_clone);
            }

            let mut sessions = sessions_clone.write().await;
            if let Some(mut session) = sessions.get_mut(&session_id_clone) {
                session.updated_at = Instant::now();
                session.ffmpeg_pid = None;

                match output {
                    FfmpegCompletion::Finished(Ok(status)) if status.success() => {
                        session.state = TranscodingSessionState::Completed;
                        session.progress = 1.0;
                        tracing::info!("transcoding session {} cancelled", session_id_clone);
                    }
                    FfmpegCompletion::Finished(Ok(status)) => {
                        let error_msg = format!(
                            "FFmpeg杩涚▼澶辫触锛岄€€鍑虹爜: {}",
                            status.code().unwrap_or(-1)
                        );
                        session.state = TranscodingSessionState::Failed(error_msg.clone());
                        session.error = Some(error_msg.clone());
                        tracing::error!("杞爜浼氳瘽 {} 澶辫触: {}", session_id_clone, error_msg);
                    }
                    FfmpegCompletion::Finished(Err(e)) => {
                        let error_msg = format!("绛夊緟FFmpeg杩涚▼澶辫触: {}", e);
                        session.state = TranscodingSessionState::Failed(error_msg.clone());
                        session.error = Some(error_msg.clone());
                        tracing::error!("杞爜浼氳瘽 {} 澶辫触: {}", session_id_clone, error_msg);
                    }
                    FfmpegCompletion::Cancelled => {
                        session.state = TranscodingSessionState::Cancelled;
                        session.progress = 0.0;
                        tracing::info!("transcoding session {} cancelled", session_id_clone);
                    }
                }
            }
        });

        Ok(session)
    }

    /// 鑾峰彇杞爜浼氳瘽
    pub async fn get_session(&self, session_id: Uuid) -> Option<TranscodingSession> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).cloned()
    }

    /// 鍋滄杞爜浼氳瘽
    pub async fn stop_transcoding(&self, session_id: Uuid) -> Result<(), AppError> {
        if let Some(sender) = self.cancellation_senders.write().await.remove(&session_id) {
            let _ = sender.send(());
        }

        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.state = TranscodingSessionState::Cancelled;
            session.updated_at = Instant::now();
            tracing::info!("鍋滄杞爜浼氳瘽: {}", session_id);
        }

        Ok(())
    }

    pub async fn stop_transcoding_for_user_device(
        &self,
        user_id: Uuid,
        device_id: Option<&str>,
    ) -> usize {
        let device_id = device_id.map(str::trim).filter(|value| !value.is_empty());
        let session_ids = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .filter(|(_, session)| {
                    session.user_id == user_id
                        && device_id.is_none_or(|device_id| session.device_id == device_id)
                        && matches!(
                            session.state,
                            TranscodingSessionState::Initializing
                                | TranscodingSessionState::Transcoding
                        )
                })
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        };

        let count = session_ids.len();
        for session_id in session_ids {
            let _ = self.stop_transcoding(session_id).await;
        }
        count
    }

    /// 娓呯悊杩囨湡鐨勮浆鐮佷細璇?
    pub async fn cleanup_expired_sessions(&self, max_age: Duration) -> usize {
        let now = Instant::now();
        let mut sessions_to_remove = Vec::new();

        // 鎵惧嚭杩囨湡浼氳瘽
        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                if now.duration_since(session.created_at) > max_age {
                    sessions_to_remove.push(*id);
                }
            }
        }

        // 绉婚櫎杩囨湡浼氳瘽
        let count = sessions_to_remove.len();
        if count > 0 {
            let mut sessions = self.sessions.write().await;
            for id in sessions_to_remove {
                sessions.remove(&id);
            }
        }

        count
    }

    /// 鏋勫缓FFmpeg鍛戒护琛屽弬鏁?
    fn build_ffmpeg_args(
        &self,
        input_path: &Path,
        output_dir: &Path,
        params: &VideoStreamQuery,
        protocol: &str,
        options: &EncodingOptionsDto,
    ) -> Result<Vec<String>, AppError> {
        let mut args = vec!["-i".to_string(), input_path.to_string_lossy().to_string()];

        // 瑙嗛缂栫爜鍙傛暟
        if let Some(video_codec) = &params.video_codec {
            args.push("-c:v".to_string());
            args.push(ffmpeg_video_encoder(video_codec, options).to_string());
        } else if protocol.eq_ignore_ascii_case("hls") {
            args.push("-c:v".to_string());
            args.push(ffmpeg_video_encoder("h264", options).to_string());
        } else {
            args.push("-c:v".to_string());
            args.push("copy".to_string()); // 榛樿澶嶅埗瑙嗛娴?
        }

        if protocol.eq_ignore_ascii_case("hls") && !options.h264_preset.trim().is_empty() {
            args.push("-preset".to_string());
            args.push(options.h264_preset.clone());
        }

        if protocol.eq_ignore_ascii_case("hls") && options.h264_crf > 0 {
            args.push("-crf".to_string());
            args.push(options.h264_crf.to_string());
        }

        // 瑙嗛鐮佺巼闄愬埗
        if let Some(max_bitrate) = params.max_video_bitrate {
            args.push("-b:v".to_string());
            args.push(format!("{}k", max_bitrate / 1000));
        }

        // 鍒嗚鲸鐜囬檺鍒?
        if params.max_width.is_some() || params.max_height.is_some() {
            let width = params.max_width.unwrap_or(1920);
            let height = params.max_height.unwrap_or(1080);
            args.push("-vf".to_string());
            args.push(format!(
                "scale='min({},iw)':'min({},ih)':force_original_aspect_ratio=decrease",
                width, height
            ));
        }

        // 闊抽缂栫爜鍙傛暟
        if let Some(audio_codec) = &params.audio_codec {
            args.push("-c:a".to_string());
            args.push(ffmpeg_audio_encoder(audio_codec).to_string());
        } else if protocol.eq_ignore_ascii_case("hls") {
            args.push("-c:a".to_string());
            args.push("aac".to_string());
        } else {
            args.push("-c:a".to_string());
            args.push("copy".to_string()); // 榛樿澶嶅埗闊抽娴?
        }

        // 闊抽澹伴亾闄愬埗
        if let Some(max_channels) = params.max_audio_channels {
            args.push("-ac".to_string());
            args.push(max_channels.to_string());
        }

        // 绾跨▼鏁?
        let threads = match options.encoding_thread_count {
            0 => num_cpus::get() as u32,
            value if value > 0 => value as u32,
            _ => (num_cpus::get() as u32 / 2).max(1),
        };
        args.push("-threads".to_string());
        args.push(threads.to_string());

        // 杈撳嚭鏍煎紡
        match protocol {
            "hls" => {
                args.push("-f".to_string());
                args.push("hls".to_string());
                args.push("-hls_time".to_string());
                args.push("6".to_string()); // 鍒嗘鏃堕暱
                args.push("-hls_list_size".to_string());
                args.push("0".to_string()); // 0琛ㄧず鏃犻檺鍒楄〃
                args.push("-hls_segment_filename".to_string());
                args.push(
                    output_dir
                        .join("segment%d.ts")
                        .to_string_lossy()
                        .to_string(),
                );
                args.push(
                    output_dir
                        .join("playlist.m3u8")
                        .to_string_lossy()
                        .to_string(),
                );
            }
            "dash" => {
                args.push("-f".to_string());
                args.push("dash".to_string());
                args.push("-seg_duration".to_string());
                args.push("6".to_string()); // 鍒嗘鏃堕暱
                args.push("-window_size".to_string());
                args.push("5".to_string()); // 绐楀彛澶у皬
                args.push("-extra_window_size".to_string());
                args.push("5".to_string()); // 棰濆绐楀彛澶у皬
                args.push(
                    output_dir
                        .join("playlist.mpd")
                        .to_string_lossy()
                        .to_string(),
                );
            }
            _ => return Err(AppError::InvalidTranscodingProtocol(protocol.to_string())),
        }

        Ok(args)
    }
}

enum FfmpegCompletion {
    Finished(std::io::Result<ExitStatus>),
    Cancelled,
}

async fn wait_for_ffmpeg_or_cancel(
    mut child: Child,
    cancel_rx: oneshot::Receiver<()>,
) -> FfmpegCompletion {
    tokio::select! {
        output = child.wait() => FfmpegCompletion::Finished(output),
        _ = cancel_rx => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            FfmpegCompletion::Cancelled
        }
    }
}

fn ffmpeg_video_encoder<'a>(codec: &'a str, options: &'a EncodingOptionsDto) -> &'a str {
    match codec.trim().to_ascii_lowercase().as_str() {
        "h264" | "avc" | "libx264" => match options
            .hardware_acceleration_type
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "nvenc" => "h264_nvenc",
            "qsv" => "h264_qsv",
            "vaapi" => "h264_vaapi",
            "h264_omx" => "h264_omx",
            _ => "libx264",
        },
        "h265" | "hevc" | "libx265" => "libx265",
        "copy" => "copy",
        _ => codec.trim(),
    }
}

fn ffmpeg_audio_encoder(codec: &str) -> &str {
    match codec.trim().to_ascii_lowercase().as_str() {
        "aac" => "aac",
        "mp3" | "libmp3lame" => "libmp3lame",
        "copy" => "copy",
        _ => codec.trim(),
    }
}
