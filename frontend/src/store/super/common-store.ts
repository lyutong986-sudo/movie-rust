/**
 * Common store base class placeholder
 */

import { reactive } from 'vue';

export class CommonStore {
  protected _state: any;
  
  constructor() {
    this._state = reactive({});
  }
  
  get state() {
    return this._state;
  }
}