export interface BridgeSequenceIdentity {
  projectId: string
  taskId: string
  sessionId: string
}

export class BridgeSequenceRegistry {
  private readonly values = new Map<string, number>()

  next(identity: BridgeSequenceIdentity): number {
    const key = bridgeSequenceKey(identity)
    const sequence = (this.values.get(key) ?? 0) + 1
    this.values.set(key, sequence)
    return sequence
  }

  accept(identity: BridgeSequenceIdentity, sequence: number): boolean {
    if (!Number.isSafeInteger(sequence) || sequence <= 0)
      return false
    const key = bridgeSequenceKey(identity)
    const previous = this.values.get(key) ?? 0
    if (sequence <= previous)
      return false
    this.values.set(key, sequence)
    return true
  }

  clear(): void {
    this.values.clear()
  }
}

function bridgeSequenceKey(identity: BridgeSequenceIdentity): string {
  return `${identity.projectId}\u241F${identity.taskId}\u241F${identity.sessionId}`
}
