use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    process::{Child, Command},
    sync::{RwLock, Semaphore},
};
use uuid::Uuid;

use crate::{config::Config, error::AppError, models::VideoStreamQuery};

/// 转码会话状态
#[derive(Debug, Clone, PartialEq)]
pub enum TranscodingSessionState {
    /// 正在初始化
    Initializing,
    /// 正在转码
    Transcoding,
    /// 已完成（HLS/DASH 文件已生成）
    Completed,
    /// 已失败
    Failed(String),
    /// 已取消
    Cancelled,
}

/// 转码会话信息
#[derive(Debug, Clone)]
pub struct TranscodingSession {
    /// 会话ID
    pub id: Uuid,
    /// 媒体项ID
    pub media_item_id: Uuid,
    /// 用户ID
    pub user_id: Uuid,
    /// 设备ID
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
    /// 转码进度（0.0-1.0）
    pub progress: f32,
    /// 错误信息（如果失败）
    pub error: Option<String>,
    /// FFmpeg 进程ID
    pub ffmpeg_pid: Option<u32>,
}

/// FFmpeg转码器
#[derive(Clone)]
pub struct Transcoder {
    /// 配置
    config: Arc<Config>,
    /// 转码会话映射表
    sessions: Arc<RwLock<HashMap<Uuid, TranscodingSession>>>,
    /// 信号量限制最大转码会话数
    session_semaphore: Arc<Semaphore>,
}

impl Transcoder {
    /// 创建新的转码器
    pub fn new(config: Arc<Config>) -> Self {
        let max_sessions = config.max_transcode_sessions.max(1);
        
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_semaphore: Arc::new(Semaphore::new(max_sessions as usize)),
        }
    }
    
    /// 开始转码会话
    pub async fn start_transcoding(
        &self,
        media_item_id: Uuid,
        user_id: Uuid,
        device_id: &str,
        params: VideoStreamQuery,
        input_path: &Path,
    ) -> Result<TranscodingSession, AppError> {
        // 检查是否启用转码
        if !self.config.enable_transcoding {
            return Err(AppError::TranscodingDisabled);
        }
        
        // 获取信号量许可（等待空闲槽位）
        let permit = self.session_semaphore.clone().acquire_owned().await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        
        // 生成会话ID和输出目录
        let session_id = Uuid::new_v4();
        let output_dir = self.config.transcode_dir
            .join(format!("session_{}", session_id));
        
        // 创建输出目录
        tokio::fs::create_dir_all(&output_dir).await
            .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        
        // 确定转码协议
        let protocol = params.transcoding_protocol
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
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, session.clone());
        }
        
        tracing::info!(
            "开始转码会话: session_id={}, media_item_id={}, protocol={}, input={:?}",
            session_id, media_item_id, protocol, input_path
        );
        
        // 更新会话状态为转码中
        {
            let mut sessions = self.sessions.write().await;
            if let Some(mut session) = sessions.get_mut(&session_id) {
                session.state = TranscodingSessionState::Transcoding;
                session.updated_at = Instant::now();
            }
        }
        
        // 构建FFmpeg命令行参数
        let ffmpeg_args = self.build_ffmpeg_args(input_path, &output_dir, &params, &protocol)?;
        let ffmpeg_path = &self.config.ffmpeg_path;
        
        // 启动FFmpeg进程
        let mut cmd = Command::new(ffmpeg_path);
        cmd.args(&ffmpeg_args);
        cmd.stdout(std::process::Stdio::null()); // 重定向标准输出
        cmd.stderr(std::process::Stdio::piped()); // 捕获标准错误用于调试
        
        tracing::debug!("执行FFmpeg命令: {} {}", ffmpeg_path, ffmpeg_args.join(" "));
        
        let child = cmd.spawn()
            .map_err(|e| {
                tracing::error!("启动FFmpeg进程失败: {}", e);
                AppError::FfmpegError(format!("启动FFmpeg进程失败: {}", e))
            })?;
        
        // 获取进程ID
        let pid = child.id();
        
        // 更新会话状态，记录进程ID
        {
            let mut sessions = self.sessions.write().await;
            if let Some(mut session) = sessions.get_mut(&session_id) {
                session.ffmpeg_pid = pid;
                session.updated_at = Instant::now();
            }
        }
        
        // 克隆必要的Arc以在异步任务中使用
        let sessions_clone = self.sessions.clone();
        let session_id_clone = session_id;
        
        // 启动后台任务监控FFmpeg进程
        tokio::spawn(async move {
            let output = child.wait_with_output().await;
            
            let mut sessions = sessions_clone.write().await;
            if let Some(mut session) = sessions.get_mut(&session_id_clone) {
                session.updated_at = Instant::now();
                session.ffmpeg_pid = None;
                
                match output {
                    Ok(output) if output.status.success() => {
                        session.state = TranscodingSessionState::Completed;
                        session.progress = 1.0;
                        tracing::info!("转码会话 {} 完成", session_id_clone);
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let error_msg = format!(
                            "FFmpeg进程失败，退出码: {}，错误: {}",
                            output.status.code().unwrap_or(-1),
                            stderr.trim()
                        );
                        session.state = TranscodingSessionState::Failed(error_msg.clone());
                        session.error = Some(error_msg.clone());
                        tracing::error!("转码会话 {} 失败: {}", session_id_clone, error_msg);
                    }
                    Err(e) => {
                        let error_msg = format!("等待FFmpeg进程失败: {}", e);
                        session.state = TranscodingSessionState::Failed(error_msg.clone());
                        session.error = Some(error_msg.clone());
                        tracing::error!("转码会话 {} 失败: {}", session_id_clone, error_msg);
                    }
                }
            }
        });
        
        Ok(session)
    }
    
    /// 获取转码会话
    pub async fn get_session(&self, session_id: Uuid) -> Option<TranscodingSession> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).cloned()
    }
    
    /// 停止转码会话
    pub async fn stop_transcoding(&self, session_id: Uuid) -> Result<(), AppError> {
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.get_mut(&session_id) {
            session.state = TranscodingSessionState::Cancelled;
            session.updated_at = Instant::now();
            
            // 在实际实现中，这里会终止FFmpeg进程
            tracing::info!("停止转码会话: {}", session_id);
        }
        
        Ok(())
    }
    
    /// 清理过期的转码会话
    pub async fn cleanup_expired_sessions(&self, max_age: Duration) -> usize {
        let now = Instant::now();
        let mut sessions_to_remove = Vec::new();
        
        // 找出过期会话
        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                if now.duration_since(session.created_at) > max_age {
                    sessions_to_remove.push(*id);
                }
            }
        }
        
        // 移除过期会话
        let count = sessions_to_remove.len();
        if count > 0 {
            let mut sessions = self.sessions.write().await;
            for id in sessions_to_remove {
                sessions.remove(&id);
            }
        }
        
        count
    }
    
    /// 构建FFmpeg命令行参数
    fn build_ffmpeg_args(
        &self,
        input_path: &Path,
        output_dir: &Path,
        params: &VideoStreamQuery,
        protocol: &str,
    ) -> Result<Vec<String>, AppError> {
        let mut args = vec!["-i".to_string(), input_path.to_string_lossy().to_string()];
        
        // 视频编码参数
        if let Some(video_codec) = &params.video_codec {
            args.push("-c:v".to_string());
            args.push(video_codec.clone());
        } else {
            args.push("-c:v".to_string());
            args.push("copy".to_string()); // 默认复制视频流
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
            args.push(format!("scale='min({},iw)':'min({},ih)':force_original_aspect_ratio=decrease", width, height));
        }
        
        // 音频编码参数
        if let Some(audio_codec) = &params.audio_codec {
            args.push("-c:a".to_string());
            args.push(audio_codec.clone());
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
        let threads = if self.config.transcode_threads > 0 {
            self.config.transcode_threads
        } else {
            // 自动检测：使用CPU核心数的一半
            num_cpus::get() as u32 / 2
        };
        args.push("-threads".to_string());
        args.push(threads.to_string());
        
        // 输出格式
        match protocol {
            "hls" => {
                args.push("-f".to_string());
                args.push("hls".to_string());
                args.push("-hls_time".to_string());
                args.push("6".to_string()); // 分段时长
                args.push("-hls_list_size".to_string());
                args.push("0".to_string()); // 0表示无限列表
                args.push("-hls_segment_filename".to_string());
                args.push(output_dir.join("segment%d.ts").to_string_lossy().to_string());
                args.push(output_dir.join("playlist.m3u8").to_string_lossy().to_string());
            },
            "dash" => {
                args.push("-f".to_string());
                args.push("dash".to_string());
                args.push("-seg_duration".to_string());
                args.push("6".to_string()); // 分段时长
                args.push("-window_size".to_string());
                args.push("5".to_string()); // 窗口大小
                args.push("-extra_window_size".to_string());
                args.push("5".to_string()); // 额外窗口大小
                args.push(output_dir.join("playlist.mpd").to_string_lossy().to_string());
            },
            _ => return Err(AppError::InvalidTranscodingProtocol(protocol.to_string())),
        }
        
        Ok(args)
    }
}