/**
 * API composables placeholder
 */

import { ref } from 'vue';

export function useBaseItem(apiConstructor: any, methodName?: string) {
  return (optionsFactory: () => any) => {
    return {
      data: ref<any>(null),
      execute: async () => {
        console.log('useBaseItem called with', apiConstructor, methodName, optionsFactory());
        return { data: null };
      }
    };
  };
}