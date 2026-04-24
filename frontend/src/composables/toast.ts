declare function useToast(): {
  add: (opts: {
    title?: string;
    description?: string;
    color?: string;
    icon?: string;
    duration?: number;
  }) => void;
};

type ToastColor = 'primary' | 'neutral' | 'success' | 'warning' | 'error' | 'info';

interface ShowToastOptions {
  title: string;
  description?: string;
  color?: ToastColor;
  icon?: string;
  timeout?: number;
}

export function useAppToast() {
  let toast: ReturnType<typeof useToast> | null = null;
  try {
    toast = useToast();
  } catch {
    toast = null;
  }

  function add(options: ShowToastOptions) {
    if (toast) {
      toast.add({
        title: options.title,
        description: options.description,
        color: options.color || 'primary',
        icon: options.icon
      });
    } else if (typeof window !== 'undefined') {
      console.info('[toast]', options.title, options.description || '');
    }
  }

  return {
    success(title: string, description?: string) {
      add({ title, description, color: 'success', icon: 'i-lucide-circle-check' });
    },
    error(title: string, description?: string) {
      add({ title, description, color: 'error', icon: 'i-lucide-triangle-alert' });
    },
    info(title: string, description?: string) {
      add({ title, description, color: 'info', icon: 'i-lucide-info' });
    },
    warn(title: string, description?: string) {
      add({ title, description, color: 'warning', icon: 'i-lucide-triangle-alert' });
    }
  };
}
