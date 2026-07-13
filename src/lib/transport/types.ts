export type Unsubscribe = () => void;

export interface Transport {
  call<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  subscribe<T>(event: string, handler: (payload: T) => void): Promise<Unsubscribe>;
  isDesktop(): boolean;
  destroy?(): void;
}
