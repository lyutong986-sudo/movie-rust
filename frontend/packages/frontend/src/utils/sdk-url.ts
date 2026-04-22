import auth from '#/plugins/remote/auth.ts';
import sdk from '#/plugins/remote/sdk/index.ts';

type PlaybackMediaSource = {
  Id?: string | null;
  Type?: string | null;
  Container?: string | null;
  SupportsDirectStream?: boolean;
  SupportsTranscoding?: boolean;
  TranscodingUrl?: string | null;
  ETag?: string | null;
  LiveStreamId?: string | null;
};

function appendPath(basePath: string, path: string): string {
  return `${basePath.replace(/\/+$/, '')}/${path.replace(/^\/+/, '')}`;
}

function authenticatedUrl(path: string, parameters: Record<string, string | undefined> = {}): string | undefined {
  const basePath = sdk.api?.basePath;
  const accessToken = auth.currentUserToken.value;

  if (!basePath || !accessToken) {
    return undefined;
  }

  const url = new URL(appendPath(basePath, path));

  url.searchParams.set('api_key', accessToken);

  for (const [key, value] of Object.entries(parameters)) {
    if (value !== undefined) {
      url.searchParams.set(key, value);
    }
  }

  return url.toString();
}

export function getSdkItemDownloadUrl(itemId: string): string | undefined {
  return authenticatedUrl(`/Items/${encodeURIComponent(itemId)}/Download`);
}

export function getSdkSystemLogUrl(name: string): string | undefined {
  return authenticatedUrl('/System/Logs/Log', { name });
}

export function getSdkSubtitleDeliveryUrl(deliveryUrl: string | undefined): string | undefined {
  const basePath = sdk.api?.basePath;

  return basePath && deliveryUrl ? appendPath(basePath, deliveryUrl) : undefined;
}

export function getSdkPlaybackStreamUrl(
  mediaSource: PlaybackMediaSource | undefined,
  mediaType: string | undefined
): string | undefined {
  const basePath = sdk.api?.basePath;
  const accessToken = auth.currentUserToken.value;

  if (
    basePath
    && accessToken
    && mediaType
    && mediaSource?.SupportsDirectStream
    && mediaSource.Type
    && mediaSource.Id
    && mediaSource.Container
  ) {
    const streamType = mediaType === 'Video' ? 'Videos' : mediaType;

    return authenticatedUrl(
      `/${streamType}/${encodeURIComponent(mediaSource.Id)}/stream.${encodeURIComponent(mediaSource.Container)}`,
      {
        Static: String(true),
        mediaSourceId: mediaSource.Id,
        deviceId: sdk.deviceInfo.id,
        Tag: mediaSource.ETag ?? '',
        LiveStreamId: mediaSource.LiveStreamId ?? ''
      }
    );
  }

  if (basePath && mediaSource?.SupportsTranscoding && mediaSource.TranscodingUrl) {
    return appendPath(basePath, mediaSource.TranscodingUrl);
  }
}

export function buildSdkWebSocketUrl(
  basePath: string | undefined,
  accessToken: string | undefined,
  deviceId: string | undefined
): string | undefined {
  if (!basePath || !accessToken || !deviceId) {
    return undefined;
  }

  const url = new URL(appendPath(basePath, '/socket'));

  url.searchParams.set('api_key', accessToken);
  url.searchParams.set('deviceId', deviceId);

  return url
    .toString()
    .replace('https:', 'wss:')
    .replace('http:', 'ws:');
}
