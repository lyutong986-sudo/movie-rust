/**
 * Simple router placeholder
 */

// In a real implementation, this would be the Vue Router instance
// For now, we create a simple object with required methods

export const router = {
  back: () => {
    console.log('Router back called');
    window.history.back();
  },
  push: (path: string) => {
    console.log(`Router push to ${path}`);
    window.location.hash = path;
  },
  replace: (path: string) => {
    console.log(`Router replace with ${path}`);
    window.location.replace(path);
  }
};