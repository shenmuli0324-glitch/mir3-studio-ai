import type { UnlistenFn } from '@tauri-apps/api/event'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

/** Rust 侧 service::plugin::watch::DshPlugin 的序列化形态（camelCase） */
export interface PluginErrorInfo {
  /** 错误消息（pnpm/运行日志片段） */
  message: string
  /** 记录动作：install / update / remove / runtime */
  action: string
  /** 记录时间（unix 秒级时间戳字符串） */
  at: string
}

/** Rust 侧 service::plugin::watch::DshPlugin 的序列化形态（camelCase） */
export interface DshPlugin {
  /** 依赖键（npm 包名），列表主键 */
  id: string
  /** 展示名：插件自身 package.json 的 name，缺失时回落预设清单 */
  name: string
  /** 已安装版本（解析失败时为空字符串） */
  version: string
  description: string
  /** 仓库地址（repository.url / homepage） */
  repo_url: string
  /** 是否在 dsh.profile.bundles 中（启动时自动加载） */
  bundled: boolean
  /** 预设清单中的「推荐」标记 */
  recommended: boolean
  /** 预设清单中的「修复」标记 */
  fix: boolean
  /** Studio 随包维护的第一方必需插件，不提供普通升级/卸载操作 */
  system: boolean
  /** 第一方插件随安装包提供的本地更新记录 */
  changelog?: string
  /** 异常信息（安装/升级/卸载失败或页面运行期上报）；undefined = 正常 */
  error?: PluginErrorInfo | null
}

export interface UseDshPluginsResult {
  plugins: DshPlugin[]
  loading: boolean
  error: string
  /** 手动重新拉取（Rust 侧也会在插件文件变化时实时推送） */
  refresh: () => Promise<void>
}

/**
 * 已安装 dsh 插件列表（react-query 实时同步）。
 *
 * 后端（`service/plugin/watch`）秒级监控 profile 插件文件（package.json +
 * node_modules 下各直接依赖清单），解析出插件元信息；首次加载走
 * `get_dsh_plugins` 查询，之后插件安装/卸载/升级/错误记录时通过
 * `dsh-plugins-updated` 事件推送完整列表，直接写入查询缓存（无需重新拉取）。
 *
 * 查询键 `['plugins']`：插件操作（升级/卸载）成功后会失效重拉，确保与后端
 * 错误注册表等非文件态数据保持一致。
 */
export function useDshPlugins(): UseDshPluginsResult {
  const queryClient = useQueryClient()

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['plugins'],
    queryFn: () => invoke<DshPlugin[]>('get_dsh_plugins'),
  })

  // 订阅后端实时推送：事件载荷即完整列表，直接写入缓存避免多余的往返拉取
  useEffect(() => {
    let unlisten: UnlistenFn | null = null
    let disposed = false

    listen<DshPlugin[]>('dsh-plugins-updated', (event) => {
      if (disposed)
        return
      queryClient.setQueryData(['plugins'], event.payload)
    })
      .then((fn) => {
        if (disposed)
          fn()
        else
          unlisten = fn
      })
      .catch((err) => {
        console.error('[useDshPlugins] failed to listen dsh-plugins-updated:', err)
      })

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [queryClient])

  return {
    plugins: data ?? [],
    loading: isLoading,
    error: error ? String(error) : '',
    refresh: async () => {
      await refetch()
    },
  }
}
