use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::Arc,
    time::Instant,
};
use dashmap::DashMap;
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

/// 转码会话状态
#[derive(Debug, Clone, PartialEq)]
pub enum TranscodingSessionState {
    /// 正在初始化
    Initializing,
    /// 姝ｅ湪杞爜
    Transcoding,
    /// 已完成（HLS/DASH 文件已生成）
    Completed,
    /// 宸插け璐?
    Failed(String),
    /// 已取消
    Cancelled,
}

/// 转码会话信息
#[derive(Debug, Clone)]
pub struct TranscodingSession {
    /// 会话 ID
    pub id: Uuid,
    /// 媒体项 ID
    pub media_item_id: Uuid,
    /// 鐢ㄦ埛ID
    pub user_id: Uuid,
    /// 璁惧ID
    pub device_id: String,
    /// 转码参数
    pub params: VideoStreamQuery,
    /// 转码协议 (HLS/DASH)
    pub protocol: String,
    /// 输出目录
    pub output_dir: PathBuf,
    /// 主播放列表文件
    pub playlist_path: PathBuf,
    /// 会话状态
    pub state: TranscodingSessionState,
    /// 创建时间
    pub created_at: Instant,
    /// 最后更新时间
    pub updated_at: Instant,
    /// 转码进度 (0.0-1.0)
    pub progress: f32,
    /// 错误信息（如果失败）
    pub error: Option<String>,
    /// FFmpeg 杩涚▼ID
    pub ffmpeg_pid: Option<u32>,
}

/// FFmpeg杞爜鍣?
#[derive(Clone)]
pub struct Transcoder {
    /// 配置
    config: Arc<Config>,
    /// 转码会话映射表
    sessions: Arc<DashMap<Uuid, TranscodingSession>>,
    /// 信号量限制最大转码会话数
    session_semaphore: Arc<Semaphore>,
    cancellation_senders: Arc<RwLock<HashMap<Uuid, oneshot::Sender<()>>>>,
}

impl Transcoder {
    /// 创建新的转码器
    pub fn new(config: Arc<Config>) -> Self {
        let max_sessions = config.max_transcode_sessions.max(1);

        Self {
            config,
            sessions: Arc::new(DashMap::new()),
            session_semaphore: Arc::new(Semaphore::new(max_sessions as usize)),
            cancellation_senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 开始转码会话
    pub async fn start_transcoding(
        &self,
        media_item_id: Uuid,
        user_id: Uuid,
        device_id: &str,
        params: VideoStreamQuery,
        options: EncodingOptionsDto,
        input_path: &Path,
    ) -> Result<TranscodingSession, AppError> {
        // 检查是否启用转码
        if !options.enable_transcoding {
            return Err(AppError::TranscodingDisabled);
        }

        // 获取信号量许可（等待空闲槽位）
        let permit = self
            .session_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // 生成会话 ID 和输出目录
        let session_id = Uuid::new_v4();
        let output_dir =
            PathBuf::from(&options.transcoding_temp_path).join(format!("session_{}", session_id));

        // 创建输出目录
        tokio::fs::create_dir_all(&output_dir)
            .await
            .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        // 确定转码协议
        let protocol = params
            .transcoding_protocol
            .as_deref()
            .unwrap_or("hls")
            .to_lowercase();

        // 构建播放列表路径
        let playlist_path = output_dir.join("playlist.m3u8");

        // 创建初始会话对象
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

        // 存储会话
        self.sessions.insert(session_id, session.clone());

        tracing::info!(
            "开始转码会话: session_id={}, media_item_id={}, protocol={}, input={:?}",
            session_id,
            media_item_id,
            protocol,
            input_path
        );

        // 更新会话状态为转码中
        if let Some(mut session) = self.sessions.get_mut(&session_id) {
            session.state = TranscodingSessionState::Transcoding;
            session.updated_at = Instant::now();
        }

        // 构建 FFmpeg 命令行参数
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
        cmd.stdout(std::process::Stdio::null()); // 重定向标准输出
        cmd.stderr(std::process::Stdio::piped()); // 捕获标准错误用于调试
        cmd.kill_on_drop(true);

        tracing::debug!(
            "执行 FFmpeg 命令: {} {}",
            ffmpeg_path,
            ffmpeg_args.join(" ")
        );

        let child = cmd.spawn().map_err(|e| {
            tracing::error!("启动 FFmpeg 进程失败: {}", e);
            AppError::FfmpegError(format!("启动 FFmpeg 进程失败: {}", e))
        })?;

        // 获取进程 ID
        let pid = child.id();

        // 更新会话状态，记录进程 ID
        if let Some(mut session) = self.sessions.get_mut(&session_id) {
            session.ffmpeg_pid = pid;
            session.updated_at = Instant::now();
        }

        let (cancel_tx, cancel_rx) = oneshot::channel();
        {
            let mut senders = self.cancellation_senders.write().await;
            senders.insert(session_id, cancel_tx);
        }

        let sessions_clone = self.sessions.clone();
        let senders_clone = self.cancellation_senders.clone();
        let session_id_clone = session_id;

        // 启动后台任务监控 FFmpeg 进程
        tokio::spawn(async move {
            let _permit = permit;
            let output = wait_for_ffmpeg_or_cancel(child, cancel_rx).await;
            {
                let mut senders = senders_clone.write().await;
                senders.remove(&session_id_clone);
            }

            if let Some(mut session) = sessions_clone.get_mut(&session_id_clone) {
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
                            "FFmpeg 进程失败，退出码: {}",
                            status.code().unwrap_or(-1)
                        );
                        session.state = TranscodingSessionState::Failed(error_msg.clone());
                        session.error = Some(error_msg.clone());
                        tracing::error!("转码会话 {} 失败: {}", session_id_clone, error_msg);
                    }
                    FfmpegCompletion::Finished(Err(e)) => {
                        let error_msg = format!("等待 FFmpeg 进程失败: {}", e);
                        session.state = TranscodingSessionState::Failed(error_msg.clone());
                        session.error = Some(error_msg.clone());
                        tracing::error!("转码会话 {} 失败: {}", session_id_clone, error_msg);
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

    /// 获取转码会话
    pub async fn get_session(&self, session_id: Uuid) -> Option<TranscodingSession> {
        self.sessions.get(&session_id).map(|entry| entry.value().clone())
    }

    /// 停止转码会话
    pub async fn stop_transcoding(&self, session_id: Uuid) -> Result<(), AppError> {
        if let Some(sender) = self.cancellation_senders.write().await.remove(&session_id) {
            let _ = sender.send(());
        }

        if let Some(mut session) = self.sessions.get_mut(&session_id) {
            session.state = TranscodingSessionState::Cancelled;
            session.updated_at = Instant::now();
            tracing::info!("停止转码会话: {}", session_id);
        }

        Ok(())
    }

    pub async fn stop_transcoding_for_user_device(
        &self,
        user_id: Uuid,
        device_id: Option<&str>,
    ) -> usize {
        let device_id = device_id.map(str::trim).filter(|value| !value.is_empty());
        let session_ids: Vec<Uuid> = self.sessions
            .iter()
            .filter(|entry| {
                let session = entry.value();
                session.user_id == user_id
                    && device_id.is_none_or(|device_id| session.device_id == device_id)
                    && matches!(
                        session.state,
                        TranscodingSessionState::Initializing
                            | TranscodingSessionState::Transcoding
                    )
            })
            .map(|entry| *entry.key())
            .collect();

        let count = session_ids.len();
        for session_id in session_ids {
            let _ = self.stop_transcoding(session_id).await;
        }
        count
    }

    #[allow(dead_code)]
    /// 构建 FFmpeg 命令行参数
    fn build_ffmpeg_args(
        &self,
        input_path: &Path,
        output_dir: &Path,
        params: &VideoStreamQuery,
        protocol: &str,
        options: &EncodingOptionsDto,
    ) -> Result<Vec<String>, AppError> {
        let mut args = vec!["-i".to_string(), input_path.to_string_lossy().to_string()];

        // 视频编码参数
        if let Some(video_codec) = &params.video_codec {
            args.push("-c:v".to_string());
            args.push(ffmpeg_video_encoder(video_codec, options).to_string());
        } else if protocol.eq_ignore_ascii_case("hls") {
            args.push("-c:v".to_string());
            args.push(ffmpeg_video_encoder("h264", options).to_string());
        } else {
            args.push("-c:v".to_string());
            args.push("copy".to_string()); // 默认复制视频流
        }

        if protocol.eq_ignore_ascii_case("hls") && !options.h264_preset.trim().is_empty() {
            args.push("-preset".to_string());
            args.push(options.h264_preset.clone());
        }

        if protocol.eq_ignore_ascii_case("hls") && options.h264_crf > 0 {
            args.push("-crf".to_string());
            args.push(options.h264_crf.to_string());
        }

        // 视频码率限制
        if let Some(max_bitrate) = params.max_video_bitrate {
            args.push("-b:v".to_string());
            args.push(format!("{}k", max_bitrate / 1000));
        }

        // 分辨率限制
        if params.max_width.is_some() || params.max_height.is_some() {
            let width = params.max_width.unwrap_or(1920);
            let height = params.max_height.unwrap_or(1080);
            args.push("-vf".to_string());
            args.push(format!(
                "scale='min({},iw)':'min({},ih)':force_original_aspect_ratio=decrease",
                width, height
            ));
        }

        // 音频编码参数
        if let Some(audio_codec) = &params.audio_codec {
            args.push("-c:a".to_string());
            args.push(ffmpeg_audio_encoder(audio_codec).to_string());
        } else if protocol.eq_ignore_ascii_case("hls") {
            args.push("-c:a".to_string());
            args.push("aac".to_string());
        } else {
            args.push("-c:a".to_string());
            args.push("copy".to_string()); // 默认复制音频流
        }

        // 音频声道限制
        if let Some(max_channels) = params.max_audio_channels {
            args.push("-ac".to_string());
            args.push(max_channels.to_string());
        }

        // 线程数
        let threads = match options.encoding_thread_count {
            0 => num_cpus::get() as u32,
            value if value > 0 => value as u32,
            _ => (num_cpus::get() as u32 / 2).max(1),
        };
        args.push("-threads".to_string());
        args.push(threads.to_string());

        // 输出格式
        match protocol {
            "hls" => {
                args.push("-f".to_string());
                args.push("hls".to_string());
                args.push("-hls_time".to_string());
                args.push("6".to_string()); // 分片时长
                args.push("-hls_list_size".to_string());
                args.push("0".to_string()); // 0 表示无限列表
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
                args.push("6".to_string()); // 分片时长
                args.push("-window_size".to_string());
                args.push("5".to_string()); // 窗口大小
                args.push("-extra_window_size".to_string());
                args.push("5".to_string()); // 额外窗口大小
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
