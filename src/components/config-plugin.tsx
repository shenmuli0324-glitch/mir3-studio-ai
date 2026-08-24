import type { DshPlugin } from '../hooks/use-dsh-plugins'
import { CircleExclamation } from '@gravity-ui/icons'
import { Button, Chip, Label, Spinner, Tooltip } from '@heroui/react'
import { useOverlay } from '@overlastic/react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { clearSafeFilesState, hasDirtySafeFiles, MIR3_SAFE_FILES_PACKAGE } from '@/features/workbench/safe-files-state'
import { store } from '@/store'
import { toast } from '@/utils'
import { useDshPlugins } from '../hooks/use-dsh-plugins'
import { Ellipsis as TextEllipsis } from './ellipsis'
import { Empty } from './empty'
import { Item } from './item'
import { Modal } from './modal'
import { PanelHeader } from './panel-header'
import { PanelState } from './panel-state'
import { PluginChangelogDialog } from './plugin-changelog-dialog'

/**
 * 「插件」面板：展示已安装插件，作为「插件出问题时」的卸载/升级入口。
 *
 * - 数据来自 `useDshPlugins`（`get_dsh_plugins` 查询 + `dsh-plugins-updated`
 *   实时事件，react-query 缓存同步）。
 * - 升级 `update_dsh_plugin` / 卸载 `remove_dsh_plugin` 已接入后端
 *   （`dsh plugin --profile <当前档案> update|remove <id>`，进程输出经
 *   `preinstall-log` 事件实时推送）。
 * - 「异常」标记：插件带 `error` 字段（安装/升级/卸载失败或页面运行期上报）
 *   时显示 danger 图标按钮，Tooltip 展示错误详情，行内可直接升级/卸载修复。
 */
export function ConfigPlugin() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const { plugins, loading, error } = useDshPlugins()

  const [dialogHolder, openDialog] = useOverlay(Modal, { type: 'holder' })
  const [changelogHolder, openChangelog] = useOverlay(PluginChangelogDialog, { type: 'holder' })

  /** 行内操作进行中状态：id + 操作类型（update/remove），保证单例运行 */
  const [busy, setBusy] = useState<{ id: string, action: 'update' | 'remove' } | null>(null)

  const upgrade = useMutation({
    mutationFn: (id: string) => invoke<void>('update_dsh_plugin', { id }),
    onSuccess: (_data, id) => {
      const name = plugins.find(p => p.id === id)?.name ?? id
      // 失效插件列表查询：dsh-plugins-updated 事件在停服务重启场景下可能丢失
      // （插件操作会停止运行中的服务），必须显式重拉以确保列表落盘后刷新。
      void queryClient.invalidateQueries({ queryKey: ['plugins'] })
      if (id === MIR3_SAFE_FILES_PACKAGE)
        clearSafeFilesState()
      toast(t('plugins.updated_toast', { name }), {})
    },
    onError: (err, id) => {
      const name = plugins.find(p => p.id === id)?.name ?? id
      console.error('[ConfigPlugin] upgrade failed:', err)
      toast(t('plugins.upgrade_failed', { name }), {})
    },
  })
  const remove = useMutation({
    mutationFn: (id: string) => invoke<void>('remove_dsh_plugin', { id }),
    onSuccess: (_data, id) => {
      const name = plugins.find(p => p.id === id)?.name ?? id
      // 同上：卸载成功后显式重拉插件列表，避免事件推送丢失导致列表未更新。
      void queryClient.invalidateQueries({ queryKey: ['plugins'] })
      toast(t('plugins.removed_toast', { name }), {})
    },
    onError: (err, id) => {
      const name = plugins.find(p => p.id === id)?.name ?? id
      console.error('[ConfigPlugin] remove failed:', err)
      toast(t('plugins.remove_failed', { name }), {})
    },
  })

  async function onUpgrade(id: string) {
    if (busy)
      return
    setBusy({ id, action: 'update' })
    try {
      await upgrade.mutateAsync(id)
    }
    catch {
      // 错误提示已由 mutation 的 onError 处理
    }
    finally {
      setBusy(null)
      // 插件操作会停掉运行中的服务（即使失败也已被后端停止），这里统一拉起服务并
      // 同步前端运行状态，避免留下「服务已死但界面仍显示运行中」的过期状态。
      void store.harness.restart()
    }
  }

  async function onRemove(id: string, name: string) {
    if (busy)
      return
    if (id === MIR3_SAFE_FILES_PACKAGE && hasDirtySafeFiles()) {
      toast(t('plugins.safe_files_dirty'), {})
      return
    }
    try {
      await openDialog({
        status: 'danger',
        title: t('plugins.remove_confirm_title'),
        description: (
          <p>
            {t('plugins.remove_confirm_desc', { name })}
          </p>
        ),
        confirmText: t('plugins.uninstall'),
      })
    }
    catch {
      return
    }
    setBusy({ id, action: 'remove' })
    try {
      await remove.mutateAsync(id)
    }
    catch {
      // 错误提示已由 mutation 的 onError 处理
    }
    finally {
      setBusy(null)
      // 同上：卸载后统一拉起服务，避免服务被后端停止后前端状态过期。
      void store.harness.restart()
    }
  }

  async function onPluginClick(plugin: DshPlugin) {
    if (!plugin.changelog)
      return
    try {
      await openChangelog({
        pluginName: plugin.name,
        version: plugin.version,
        changelog: plugin.changelog ?? '',
        system: plugin.system,
      })
    }
    catch {
      // 用户关闭更新记录弹窗，无需额外处理。
    }
  }

  return (
    <div>
      <PanelHeader className="sticky top-0 bg-canvas z-10 pb-3" title={t('plugins.title')} description={t('plugins.panel_tooltip')} />

      {/* 加载 / 失败 / 空态 */}
      <PanelState loading={loading} error={error}>
        <If
          cond={plugins.length > 0}
          else={(
            <Empty>{t('plugins.empty')}</Empty>
          )}
        >
          <div className="space-y-3 flex-wrap gap-2">
            {plugins.map(plugin => (
              <Item
                key={plugin.id}
                interactive={plugin.changelog != null}
                onClick={() => void onPluginClick(plugin)}
                left={(
                  <div className="min-w-0">
                    <div className="flex min-w-0 items-center gap-1">
                      <If cond={plugin.error != null}>
                        <Tooltip delay={0}>
                          <Button
                            isIconOnly
                            size="sm"
                            variant="ghost"
                            className="size-6 shrink-0 rounded-md text-danger"
                            aria-label={t('plugins.abnormal_tooltip')}
                          >
                            <CircleExclamation />
                          </Button>
                          <Tooltip.Content className="max-w-[320px]">
                            <div className="space-y-1">
                              <p className="text-xs font-medium">
                                {t('plugins.abnormal_desc', { name: plugin.name })}
                              </p>
                              <p className="whitespace-pre-wrap break-all font-mono text-[11px] opacity-80">
                                {plugin.error?.message}
                              </p>
                            </div>
                          </Tooltip.Content>
                        </Tooltip>
                      </If>
                      <Label className="min-w-0 truncate text-sm font-medium text-ink">
                        {plugin.name}
                      </Label>
                      <If cond={plugin.version !== ''}>
                        <code className="shrink-0 rounded bg-default px-1.5 py-0.5 font-mono text-[10px] text-muted">
                          {plugin.version}
                        </code>
                      </If>
                    </div>
                    <If cond={plugin.description !== ''}>
                      <TextEllipsis lineClamp={2} className="text-xs text-muted">
                        {plugin.description}
                      </TextEllipsis>
                    </If>
                  </div>
                )}
                right={(
                  <>
                    <If cond={plugin.system}>
                      <Chip className="rounded-md" variant="soft" color="accent" size="sm">
                        {t('plugins.system')}
                      </Chip>
                    </If>
                    <If cond={plugin.changelog != null}>
                      <Chip className="rounded-md" variant="primary" color="accent" size="sm">
                        {t('plugins.view_changelog')}
                      </Chip>
                    </If>
                    {/* 系统插件由 Studio 随版本维护，普通管理界面不提供变更入口。 */}
                    <If cond={!plugin.system && plugin.error != null}>
                      <Chip
                        className={`rounded-md${busy ? ' cursor-not-allowed opacity-50' : ' cursor-pointer'}`}
                        variant="primary"
                        color="accent"
                        size="sm"
                        onClick={() => onUpgrade(plugin.id)}
                      >
                        <span className="flex items-center gap-1">
                          <If cond={busy?.id === plugin.id && busy.action === 'update'} then={<Spinner size="sm" color="current" />} />
                          {t('plugins.upgrade')}
                        </span>
                      </Chip>
                    </If>
                    <If cond={!plugin.system}>
                      <Chip
                        className={`rounded-md${busy ? ' cursor-not-allowed opacity-50' : ' cursor-pointer'}`}
                        variant="primary"
                        color="danger"
                        size="sm"
                        onClick={() => onRemove(plugin.id, plugin.name)}
                      >
                        <span className="flex items-center gap-1">
                          <If cond={busy?.id === plugin.id && busy.action === 'remove'} then={<Spinner size="sm" color="current" />} />
                          {t('plugins.uninstall')}
                        </span>
                      </Chip>
                    </If>
                  </>
                )}
              />
            ))}
          </div>
        </If>
      </PanelState>

      {dialogHolder}
      {changelogHolder}
    </div>
  )
}
