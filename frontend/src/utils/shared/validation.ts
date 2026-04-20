/**
 * Simple validation utilities to replace @jellyfin-vue/shared/validation
 */

export const isNil = (value: unknown): value is null | undefined => {
  return value === null || value === undefined;
};

export const isFunc = (value: unknown): value is (...args: any[]) => any => {
  return typeof value === 'function';
};

export const isObj = (value: unknown): value is Record<string, any> => {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
};

export const isNumber = (value: unknown): value is number => {
  return typeof value === 'number' && !isNaN(value);
};

export function sealed(constructor: Function) {
  Object.seal(constructor);
  Object.seal(constructor.prototype);
}