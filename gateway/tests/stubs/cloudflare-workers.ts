// Test-only stub for the cloudflare:workers runtime module. It provides just
// the DurableObject base class that the coordinator extends, so the worker
// module can be imported under vitest, which cannot resolve the real virtual
// module. The production runtime uses the real class.
export class DurableObject<E = unknown> {
  protected ctx: any;
  protected env: E;
  constructor(ctx: any, env: E) {
    this.ctx = ctx;
    this.env = env;
  }
}
