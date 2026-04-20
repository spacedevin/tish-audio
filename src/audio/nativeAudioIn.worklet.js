// Pulls native PCM (posted from the main thread) into the audio graph via AudioWorklet.
// Replaces ScriptProcessorNode, which is unreliable in WKWebView when dev loads from http://127.0.0.1.

const RING_LOG = 18;
const RING_SIZE = 1 << RING_LOG;
const RING_MASK = RING_SIZE - 1;

class NativeAudioInProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.buf = new Float32Array(RING_SIZE);
    /** @type {number} next logical index to write */
    this.writeL = 0;
    /** @type {number} fractional read position (logical sample index) */
    this.readPos = 0;
    /** @type {number} nativeRate / contextSampleRate */
    this.ratio = 1;
    this._diagChunkPosted = false;
    this._diagProcessPosted = false;

    this.port.onmessage = (e) => {
      const d = e.data;
      if (!d || typeof d.type !== "string") return;
      if (d.type === "config") {
        const r = d.ratio;
        this.ratio = typeof r === "number" && r > 0 ? r : 1;
        return;
      }
      if (d.type === "reset") {
        this.writeL = 0;
        this.readPos = 0;
        this.buf.fill(0);
        return;
      }
      if (d.type === "chunk") {
        const s = d.samples;
        if (!(s instanceof Float32Array) || s.length === 0) return;

        if (!this._diagChunkPosted) {
          this._diagChunkPosted = true;
          let maxAbs = 0;
          for (let i = 0; i < s.length; i++) {
            const a = Math.abs(s[i]);
            if (a > maxAbs) maxAbs = a;
          }
          this.port.postMessage({ type: "diagChunkMaxAbs", maxAbs });
        }

        const len = s.length;
        while (this.writeL - Math.floor(this.readPos) >= RING_SIZE - 1024) {
          this.readPos = this.writeL - 2000;
        }
        let w = this.writeL;
        for (let j = 0; j < len; j++) {
          this.buf[w & RING_MASK] = s[j];
          w++;
        }
        this.writeL = w;
      }
    };
  }

  process(inputs, outputs) {
    const out = outputs[0][0];
    const ratio = this.ratio;
    const buf = this.buf;
    const mask = RING_MASK;
    let readPos = this.readPos;

    for (let i = 0; i < out.length; i++) {
      const floorR = Math.floor(readPos);
      const avail = this.writeL - floorR;
      if (avail < 2) {
        out[i] = 0;
      } else {
        const p0 = floorR & mask;
        const p1 = (floorR + 1) & mask;
        const rf = readPos - floorR;
        const s0 = buf[p0];
        const s1 = buf[p1];
        out[i] = s0 * (1 - rf) + s1 * rf;
        readPos += ratio;
      }
    }

    this.readPos = readPos;

    if (!this._diagProcessPosted) {
      this._diagProcessPosted = true;
      this.port.postMessage({ type: "diagFirstProcess" });
    }

    return true;
  }
}

registerProcessor("native-audio-in", NativeAudioInProcessor);
