/**
 * Image utilities placeholder
 */

export function getImageInfo(item: any, options?: { preferBackdrop?: boolean }) {
  // Simple implementation
  return {
    url: item?.BackdropImageTags?.[0] 
      ? `/Items/${item.Id}/Images/Backdrop/0` 
      : item?.PrimaryImageTag
        ? `/Items/${item.Id}/Images/Primary/0`
        : '/placeholder.jpg'
  };
}