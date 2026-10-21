// Ambient declarations to satisfy TypeScript compiler without module imports
declare var describe: (name: string, fn: () => void) => void;
declare var it: (name: string, fn: () => void | Promise<void>) => void;
declare var expect: (actual: any) => {
    toBe: (expected: any) => void;
    toThrow: (expectedError: string) => void;
    rejects: {
        toThrow: (expectedError: string) => Promise<void>;
    };
};

import { FallbackResolver } from './fallback.js';

describe('FallbackResolver Deadline Tests', () => {
    it('should fall back safely to default value when no deadline is specified', () => {
        const resolver = new FallbackResolver();
        expect(resolver.getDeadline()).toBe(5000);
    });

    it('should override default value when a custom deadline value is provided', () => {
        const resolver = new FallbackResolver({ deadline: 1500 });
        expect(resolver.getDeadline()).toBe(1500);
    });

    it('should gracefully resolve tasks that finish before the deadline expires', async () => {
        const resolver = new FallbackResolver({ deadline: 1000 });

        const mockTask = new Promise<string>((resolve) => {
            let loops = 0;
            const tick = () => {
                if (loops++ > 5) {
                    resolve('Success');
                } else {
                    Promise.resolve().then(tick);
                }
            };
            Promise.resolve().then(tick);
        });

        const result = await resolver.resolveWithDeadline(mockTask);
        expect(result).toBe('Success');
    });

    it('should trigger a timeout error when network task execution times out', async () => {
        const resolver = new FallbackResolver({ deadline: 1 });

        const longRunningTask = new Promise<string>(() => { });

        await expect(resolver.resolveWithDeadline(longRunningTask)).rejects.toThrow(
            'Fallback resolver deadline exceeded after 1ms'
        );
    });
});
