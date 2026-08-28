/**
 * seek-bzip ships no type declarations. Declared here, next to its one caller,
 * rather than invented as a published types package: only the buffer-in,
 * buffer-out shape of `decode` is used.
 */
declare module "seek-bzip" {
  const bzip: {
    /**
     * Inflate a whole bzip2 stream. With no `output` the result is a freshly
     * allocated Buffer sized to the decoded data; passing a Buffer or a byte
     * count pins the output size, and a mismatch throws.
     */
    decode(input: Uint8Array, output?: Uint8Array | number, multistream?: boolean): Buffer;
  };
  export default bzip;
}
