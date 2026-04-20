/**
 * Simple remote placeholder for Jellyfin SDK
 */

import { ref } from 'vue';

// Simple reactive stores
export const auth = {
  currentUserId: ref<string | null>(null),
  currentUserToken: ref<string | null>(null),
  currentUser: ref<any>(null),
  onBeforeLogout: (callback: () => void) => {
    console.log('onBeforeLogout registered');
    // Simple implementation
  }
};

// SDK placeholder
export const sdk = {
  api: {
    basePath: 'http://localhost:8096',
    axiosInstance: null as any
  },
  deviceInfo: {
    id: 'movie-rust-web-client'
  },
  newUserApi: (apiConstructor: any) => {
    // Return a mock API object
    return {
      getPostedPlaybackInfo: async () => ({ data: {} }),
      reportPlaybackProgress: async () => {},
      reportPlaybackStopped: async () => {},
      reportPlaybackStart: async () => {}
    };
  }
};

export const remote = {
  auth,
  sdk
};