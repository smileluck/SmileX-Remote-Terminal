/**
 * 环境管理面板类型定义
 *
 * 字段对齐 services/env.ts 的探测输出（SRT_ENV|<id>|<installed>|<version>|
 * <path>|<service>|<config>|<bin> 分隔行逐行解析，bin 字段可选）。
 */

/** 支持管理的环境 ID */
export type EnvId =
  | 'python'
  | 'go'
  | 'java'
  | 'mysql'
  | 'postgresql'
  | 'redis'
  | 'nginx'
  | 'openresty'

/** 环境操作错误分类 */
export type EnvErrorKind = 'not-installed' | 'no-permission' | 'unsupported-os' | 'unknown'

/** 服务启停动作（仅服务类环境） */
export type ServiceAction = 'start' | 'stop' | 'restart'

/** 单个环境的探测状态 */
export interface EnvStatus {
  id: EnvId
  /** 是否已安装 */
  installed: boolean
  /** 版本号（未安装为 ''） */
  version: string
  /** 安装目录（非二进制路径；未安装为 ''） */
  installPath: string
  /** 服务运行状态（仅服务类环境） */
  serviceActive?: 'active' | 'inactive' | 'failed' | 'unknown'
  /** systemctl unit 名（仅服务类环境） */
  serviceName?: string
  /** 探测到的主配置文件路径（无配置文件的环境为空） */
  configPath?: string
  /** 二进制所在目录（仅 nginx 等路径已指到配置目录的环境提供，供跳转选择） */
  binPath?: string
  /** 安装来源：pyenv / sdkman / official / system（仅 python/go/java 探测） */
  source?: string
}
