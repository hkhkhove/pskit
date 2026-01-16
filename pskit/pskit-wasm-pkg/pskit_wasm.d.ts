/* tslint:disable */
/* eslint-disable */
export function annotate_binding_pairs(input: Uint8Array, cutoff: number, format: string): BindingPairs;
export function split_complex(input: Uint8Array, format: string): Chunks;
export function split_by_chain(input: Uint8Array, format: string): Chunks;
export function extract_fragment(input: Uint8Array, chain_id: string, start: number | null | undefined, end: number | null | undefined, format: string): Fragment;
export function d_map(input: Uint8Array, chain_id: string | null | undefined, format: string): ContactMap;
export class BindingPairs {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Take the pairs array (consuming).
   */
  take_pairs(): Array<any> | undefined;
  /**
   * Take the distances as Float64Array (consuming).
   */
  take_distances(): Float64Array | undefined;
}
export class Chunks {
  free(): void;
  [Symbol.dispose](): void;
  constructor();
  keys(): Array<any>;
  /**
   * Take (remove) a chunk by key and return it as JS Uint8Array (owned by JS).
   *
   * This is a "consuming" API: once taken, it is removed from WASM memory.
   * Safe to keep the returned Uint8Array across async boundaries.
   */
  take(key: string): Uint8Array;
  readonly len: number;
}
export class ContactMap {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Take the values as a flat Float64Array (consuming, row-major order).
   */
  take_values(): Float64Array | undefined;
  /**
   * Take the axis labels (consuming).
   */
  take_axis(): Array<any> | undefined;
}
export class Fragment {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Take the bytes out as a JS Uint8Array (consuming, avoids double memory).
   * Returns None if already taken.
   */
  take_bytes(): Uint8Array | undefined;
  readonly end: number;
  readonly start: number;
}
