export interface CoreCandidateCanaryDependencies {
  runCanary: () => Promise<void>
  markReady: () => Promise<void>
  rollback: () => Promise<boolean>
  relaunch: () => Promise<void>
  refresh: () => void
}

export type CoreCandidateCanaryOutcome
  = | { status: 'committed' }
    | { status: 'rejected', error: unknown, rolledBack: boolean }

export async function runCoreCandidateCanary(dependencies: CoreCandidateCanaryDependencies): Promise<CoreCandidateCanaryOutcome> {
  try {
    await dependencies.runCanary()
    await dependencies.markReady()
    return { status: 'committed' }
  }
  catch (error) {
    const rolledBack = await dependencies.rollback()
    if (rolledBack) {
      await dependencies.relaunch()
      dependencies.refresh()
    }
    return { status: 'rejected', error, rolledBack }
  }
}
