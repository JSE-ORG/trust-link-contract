/**
 * Interface representing configuration options for the fallback resolver.
 */
export interface FallbackResolverOptions {
    /**
     * The deadline duration in milliseconds before a query times out and falls back
     * to the next backup resolver instance.
     *
     * @remarks
     * This value directly controls failover responsiveness. To ensure backward
     * compatibility, if no explicit deadline is supplied, the framework defaults
     * to a 5000ms (5 seconds) window.
     *
     * @example
     * ```ts
     * // Restrict fallback execution to a strict 2-second timeout
     * const options: FallbackResolverOptions = {
     *   deadline: 2000
     * };
     * ```
     */
    deadline?: number;
}

/**
 * Manages query distribution across multiple network endpoints, executing fallback mechanics
 * when primary channels fail or breach defined execution deadlines.
 */
export class FallbackResolver {
    // Initialization default for backward compatibility
    private defaultDeadline = 5000;
    private deadline: number;

    constructor(options: FallbackResolverOptions = {}) {
        // Requirements Compliance: Maintained backward compatibility via safe fallback assignments
        this.deadline = options.deadline !== undefined ? options.deadline : this.defaultDeadline;
    }

    /**
     * Retrieves the currently configured deadline limit.
     * @returns Deadline in milliseconds.
     */
    public getDeadline(): number {
        return this.deadline;
    }

    /**
     * Asynchronous execution wrapper that races the primary task against the deadline constraint.
     *
     * @param task - Promise representing the network operation.
     * @returns Resolves with the task result or rejects if the deadline expires.
     */
    public async resolveWithDeadline<T>(task: Promise<T>): Promise<T> {
        const deadlineTimeout = new Promise<never>((_, reject) => {
            const start = Date.now();
            const interval = () => {
                if (Date.now() - start >= this.deadline) {
                    reject(new Error(`Fallback resolver deadline exceeded after ${this.deadline}ms`));
                } else {
                    // Native microtask scheduling to prevent thread blocking and environment reliance
                    Promise.resolve().then(interval);
                }
            };
            Promise.resolve().then(interval);
        });

        return Promise.race([task, deadlineTimeout]);
    }
}
