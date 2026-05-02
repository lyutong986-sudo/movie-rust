// PB49 (UX)：远端 Emby sync / scanner / scheduled-task 三类 phase 的统一中文化映射。
//
// 原本同一段 switch/case 散落在 LibrarySettings / RemoteEmbySettings / ScheduledTasksSettings
// 三个文件里，三处都要维护一份；新增一个 phase（比如 WaitingForGlobalSlot）就要改三个地方
// 还容易漏改。统一收到这里之后，三个页面都引这个函数即可。

export interface SyncPhaseLabelOptions {
  /// 是否在「远端阶段」前面拼上 "远端 · " 前缀。LibrarySettings 用 true（区分本地阶段），
  /// RemoteEmbySettings 用 false（页面本身就在「远端 Emby」上下文里，不重复修饰）。
  remotePrefix?: boolean;
}

export function syncPhaseLabel(
  phase: string | null | undefined,
  options: SyncPhaseLabelOptions = {}
): string {
  if (!phase) return '';
  const prefix = options.remotePrefix !== false;

  // 后端在自动重试时会把 phase 设成 Retrying(N/M)。
  const retry = /^Retrying\((\d+)\/(\d+)\)$/.exec(phase);
  if (retry) return `重试中 ${retry[1]}/${retry[2]}`;

  // incremental_update_library 在跑远端 sync 时会把 scanner phase 套上 RemoteSync/ 前缀。
  // 直接传入「裸 RemoteSync 阶段名」（如 RemoteEmbySettings 直接拿后端 phase 字段时）也走这条路。
  let bare = phase;
  let needPrefix = false;
  if (phase.startsWith('RemoteSync/')) {
    bare = phase.slice('RemoteSync/'.length);
    needPrefix = prefix;
  }

  switch (bare) {
    case 'WaitingForGlobalSlot':
      // PB49 (Cap)：等待全局远端 sync 并发槽（多源同时触发时排队）。
      return needPrefix ? '远端 · 队列等待中' : '队列等待中';
    case 'Preparing':
      return needPrefix ? '远端 · 准备中' : '准备中';
    case 'CountingRemoteItems':
      return needPrefix ? '远端 · 统计远端总数' : '统计远端总数';
    case 'FetchingRemoteIndex':
      return needPrefix ? '远端 ID 索引中' : 'ID 索引中';
    case 'FetchingRemoteItems':
      return needPrefix ? '远端条目获取中' : '条目获取中';
    case 'SyncingRemoteItems':
    case 'UpsertingVirtualItems':
      return needPrefix ? '远端条目入库中' : '条目入库中';
    case 'PruningStaleItems':
      return needPrefix ? '清理远端已删条目' : '清理已删条目';
    case 'FinalizingSeriesDetails':
      return needPrefix ? '剧集元数据收尾' : '元数据收尾';
    case 'CollectingFiles':
      return '收集文件中';
    case 'Importing':
      return '入库中';
    case 'PostProcessing':
      return '后处理';
    case 'Completed':
      return needPrefix ? '远端同步已完成' : '已完成';
    case 'Cancelled':
      return '已取消';
    case 'Failed':
      return '已失败';
    case 'Queued':
      return '排队中';
    default:
      // 未知 phase：保持原样而不是返回空，至少能让用户看到后端真实状态。
      return needPrefix && bare !== phase ? `远端 · ${bare}` : bare;
  }
}
