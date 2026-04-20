/**
 * Simple snackbar utility for displaying notifications
 */

export function useSnackbar(message: string, type: 'error' | 'success' | 'info' = 'info'): void {
  console.log(`[${type.toUpperCase()}] ${message}`);
  
  // In a real implementation, this would show a UI notification
  // For now, we just log to console
  if (type === 'error') {
    console.error(message);
  } else if (type === 'success') {
    console.log(message);
  } else {
    console.info(message);
  }
}