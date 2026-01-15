/* tslint:disable */
/* eslint-disable */
export function d_map(input: Uint8Array, chain_id: string | null | undefined, format: string): ContactMap;
export function split_complex(input: Uint8Array, format: string): Chunks;
export function split_by_chain(input: Uint8Array, format: string): Chunks;
export function annotate_binding_pairs(input: Uint8Array, cutoff: number, format: string): BindingPairs;
export function extract_fragment(input: Uint8Array, chain_id: string, start: number | null | undefined, end: number | null | undefined, format: string): Fragment;
export class BindingPairs {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  pairs(): Array<any>;
  distances(): Float64Array;
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
  axis(): Array<any>;
  values(): Float64Array;
}
export class Fragment {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  end(): number;
  bytes(): Uint8Array;
  start(): number;
}
