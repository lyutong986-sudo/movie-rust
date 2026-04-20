import type { BaseItemDto } from '../api/emby';

type ItemLike = Pick<BaseItemDto, 'Id' | 'Type' | 'IsFolder'>;

export function itemRoute(item: ItemLike) {
  if (item.Type === 'CollectionFolder') {
    return `/library/${item.Id}`;
  }

  if (item.Type === 'Series') {
    return `/series/${item.Id}`;
  }

  return `/item/${item.Id}`;
}

export function playbackRoute(item: ItemLike) {
  if (item.IsFolder) {
    return itemRoute(item);
  }

  const path = item.Type === 'Audio' || item.Type === 'MusicAlbum' ? '/playback/music' : '/playback/video';
  const params = new URLSearchParams({
    itemId: item.Id
  });

  return `${path}?${params}`;
}

export function genreRoute(name: string, type?: string) {
  const params = new URLSearchParams();
  if (type) {
    params.set('type', type);
  }

  const suffix = params.size ? `?${params}` : '';
  return `/genre/${encodeURIComponent(name)}${suffix}`;
}
