/**
 * Simple PromiseQueue implementation to replace @jellyfin-vue/shared/promises
 */

export class PromiseQueue {
  private queue: (() => Promise<any>)[] = [];
  private running = false;

  async add<T>(task: () => Promise<T>): Promise<T> {
    return new Promise((resolve, reject) => {
      this.queue.push(async () => {
        try {
          const result = await task();
          resolve(result);
        } catch (error) {
          reject(error);
        }
      });
      
      if (!this.running) {
        this.run();
      }
    });
  }

  private async run(): Promise<void> {
    if (this.running || this.queue.length === 0) {
      return;
    }

    this.running = true;
    
    while (this.queue.length > 0) {
      const task = this.queue.shift();
      if (task) {
        await task();
      }
    }
    
    this.running = false;
  }

  clear(): void {
    this.queue = [];
  }

  get length(): number {
    return this.queue.length;
  }
}