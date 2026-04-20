/**
 * Item utilities placeholder
 */

export function getItemRuntime(item: any): number {
  // Return runtime in minutes or 0
  return item?.RunTimeTicks ? item.RunTimeTicks / 10000000 / 60 : 0;
}