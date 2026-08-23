import type { UnlistenFn } from '@tauri-apps/api/event'
import type { DshUpdateInfo } from './types'
import { invoke } from '@tauri-apps/api/core'
import i18next, { t } from 'i18next'
import { defineStore } from 'valtio-define'
import { toast } from '@/utils'
import { harness } from '../harness'

/**
 * 版本更新模块：后台静默检查 + 手动更新安装。
 * 安装进度与启动等待复用 harness 模块的能力，本模块只负责"有没有新版本"的决策。
 */
export const harnessUpdater = defineStore({
  state: () => ({
    /** 发现的新版本信息（null 表示暂无/已被忽略） */
    updateInfo: null as DshUpdateInfo | null,
    /** 是否正在安装更新 */
    updating: false,
  }),
  actions: {
    /** 后台静默检查是否有新版 MIR3 AI Core（网络失败/API 限流时静默跳过） */
    async checkForUpdate() {
      try {
        const info = await invoke<DshUpdateInfo | null>('check_dsh_update')
        if (info) {
          this.updateInfo = info
        }
      }
      catch (err) {
        console.warn('[MIR3 AI Core] update check skipped:', err)
      }
    },

    /** 手动更新：重新下载安装新版并重启服务 */
    async handleUpdate() {
      if (this.updating)
        return
      this.updating = true
      let unlistenInstall: UnlistenFn | null = null
      try {
        unlistenInstall = await harness.listenInstallProgress()
        harness.prepareInstall(i18next.t('status.updating'))
        // 返回是否真正落盘更新：false 表示未发生安装（已是最新，或 GitHub
        // 限流拿不到可信摘要而保持本地安装）。此时绝不重启页面、也不丢弃
        // “有新版本”提示——否则用户会看到“页面刷新了、版本却没变、提示也没了”。
        const changed = await invoke<boolean>('install_dependencies')
        if (!changed) {
          // 回到就绪态（若当前停在安装/更新界面，则恢复到原 iframe 视图）
          harness.status = 'ready'
          // 重新核对一次：若确有新版，恢复“存在新版本”的提示并明确告知无法校验，
          // 而非静默当作“已更新”成功。
          this.updateInfo = null
          try {
            await this.checkForUpdate()
          }
          catch {
            /* 核对失败保持 updateInfo 为空，交由下次轮询恢复 */
          }
          if (this.updateInfo) {
            toast(i18next.t('update.verify_failed'), { variant: 'danger' })
          }
          return
        }
        await harness.launchAndWait()
        this.updateInfo = null
      }
      catch (err) {
        console.error('[MIR3 AI Core] update failed:', err)
        harness.fail(String(err))
      }
      finally {
        unlistenInstall?.()
        this.updating = false
      }
    },

    /** 忽略本次更新提示 */
    dismissUpdate() {
      this.updateInfo = null
    },

    showToast() {
      if (!this.updateInfo)
        return
      toast(t('update.available', { tag: this.updateInfo.tag }), {
        actionProps: {
          children: t('update.now'),
          onPress: () => {
            toast.clear()
            void this.handleUpdate()
          },
          variant: 'tertiary',
        },
        placement: 'bottom end',
        description: this.updateInfo.commit.slice(0, 7),
        variant: 'default',
      })
    },
  },
})
