import { useWatch } from '@hairy/react-lib'
import { useStore } from 'valtio-define'
import { store } from '@/store'

/** 右下角"发现新版本"提示条：状态与操作直接来自 updater store */
export function HarnessUpdater() {
  const { updateInfo, updating } = useStore(store.harnessUpdater)

  useWatch([updateInfo, updating], () => {
    if (!updateInfo || updating)
      return null
    store.harnessUpdater.showToast()
  }, { immediate: true })

  return null
}
