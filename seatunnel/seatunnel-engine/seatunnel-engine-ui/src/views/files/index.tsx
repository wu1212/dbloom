/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

import { computed, defineComponent, h, ref } from 'vue'
import {
  NBreadcrumb,
  NBreadcrumbItem,
  NButton,
  NDataTable,
  NIcon,
  NLayout,
  NLayoutContent,
  NModal,
  NPopconfirm,
  NRadioButton,
  NRadioGroup,
  NSpace,
  NTabPane,
  NTabs,
  NTag,
  NUpload,
  useMessage
} from 'naive-ui'
import type {
  DataTableColumns,
  UploadCustomRequestOptions,
  UploadInst
} from 'naive-ui'
import {
  CloudUploadOutline,
  DocumentTextOutline,
  FolderOutline,
  HomeOutline,
  RefreshOutline
} from '@vicons/ionicons5'
import { useI18n } from 'vue-i18n'
import { filesService } from '@/service/files'
import type { FileType, UploadedFile } from '@/service/files/types'

type TypeTab = 'all' | FileType

type Entry =
  | { kind: 'dir'; key: string; name: string; path: string; fileTypes: FileType[] }
  | { kind: 'file'; key: string; name: string; file: UploadedFile }

const formatSize = (size: number) => {
  if (size < 1024) {
    return `${size} B`
  }
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = size / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  return `${value.toFixed(1)} ${units[unit]}`
}

export default defineComponent({
  setup() {
    const { t } = useI18n()
    const message = useMessage()

    const loading = ref(false)
    const allFiles = ref([] as UploadedFile[])
    const typeTab = ref<TypeTab>('all')
    const currentDir = ref('')
    const showModal = ref(false)
    const uploadType = ref<FileType>('file')
    const uploadRef = ref<UploadInst | null>(null)

    const fetch = async () => {
      loading.value = true
      try {
        allFiles.value = await filesService.listFiles()
      } catch (e) {
        message.error(t('files.load_failed'))
      } finally {
        loading.value = false
      }
    }
    fetch()

    const segments = computed(() =>
      currentDir.value ? currentDir.value.split('/') : []
    )

    const entries = computed<Entry[]>(() => {
      const filtered = allFiles.value.filter(
        (f) => typeTab.value === 'all' || f.fileType === typeTab.value
      )
      const prefix = currentDir.value ? `${currentDir.value}/` : ''
      const dirTypes = new Map<string, Set<FileType>>()
      const files: UploadedFile[] = []
      filtered.forEach((f) => {
        if (!f.fileName.startsWith(prefix)) {
          return
        }
        const rest = f.fileName.substring(prefix.length)
        const sep = rest.indexOf('/')
        if (sep >= 0) {
          const dirName = rest.substring(0, sep)
          const types = dirTypes.get(dirName) ?? new Set<FileType>()
          types.add(f.fileType as FileType)
          dirTypes.set(dirName, types)
        } else {
          files.push(f)
        }
      })
      const result: Entry[] = Array.from(dirTypes.keys())
        .sort()
        .map((name) => ({
          kind: 'dir',
          key: `dir:${prefix}${name}`,
          name,
          path: `${prefix}${name}`,
          fileTypes: Array.from(dirTypes.get(name)!)
        }))
      files
        .sort((a, b) => a.fileName.localeCompare(b.fileName))
        .forEach((f) => {
          result.push({
            kind: 'file',
            key: `file:${f.fileType}:${f.fileName}`,
            name: f.fileName.substring(prefix.length),
            file: f
          })
        })
      return result
    })

    const openDir = (path: string) => {
      currentDir.value = path
    }

    const onUpdateTab = (value: string) => {
      typeTab.value = value as TypeTab
      currentDir.value = ''
    }

    const download = (file: UploadedFile) => {
      window.open(
        filesService.buildDownloadUrl(file.fileName, file.fileType as FileType),
        '_blank'
      )
    }

    const remove = async (file: UploadedFile) => {
      try {
        await filesService.deleteFile(file.fileName, file.fileType as FileType)
        message.success(t('files.delete_success'))
        fetch()
      } catch (e) {
        message.error(t('files.delete_failed'))
      }
    }

    // A virtual folder may be backed by files in both storage dirs (file & jar) when the
    // "all" tab is active, so delete it in every storage dir it lives in.
    const removeDir = async (dir: Extract<Entry, { kind: 'dir' }>) => {
      try {
        for (const fileType of dir.fileTypes) {
          await filesService.deleteFile(dir.path, fileType, true)
        }
        message.success(t('files.delete_success'))
        if (
          currentDir.value === dir.path ||
          currentDir.value.startsWith(`${dir.path}/`)
        ) {
          const parent = dir.path.substring(0, dir.path.lastIndexOf('/'))
          currentDir.value = parent
        }
        fetch()
      } catch (e) {
        message.error(t('files.delete_failed'))
        fetch()
      }
    }

    const openUpload = () => {
      uploadType.value = typeTab.value === 'all' ? 'file' : typeTab.value
      showModal.value = true
    }

    const customRequest = (options: UploadCustomRequestOptions) => {
      const rawFile = options.file.file
      if (!rawFile) {
        options.onError()
        return
      }
      const targetPath = currentDir.value
        ? `${currentDir.value}/${rawFile.name}`
        : rawFile.name
      filesService
        .uploadFile(rawFile, uploadType.value, targetPath, (progressEvent: any) => {
          if (progressEvent.total) {
            options.onProgress({
              percent: Math.ceil((progressEvent.loaded / progressEvent.total) * 100)
            })
          }
        })
        .then(() => {
          message.success(t('files.upload_success'))
          options.onFinish()
          uploadRef.value?.clear()
          fetch()
        })
        .catch(() => {
          message.error(t('files.upload_failed'))
          options.onError()
        })
    }

    const createColumns = (): DataTableColumns<Entry> => [
      {
        title: t('files.name'),
        key: 'name',
        render(row: Entry) {
          if (row.kind === 'dir') {
            return (
              <NSpace align="center" size={6}>
                <NIcon component={FolderOutline} color="#f0a020" />
                <NButton text onClick={() => openDir(row.path)}>
                  {row.name}
                </NButton>
              </NSpace>
            )
          }
          return (
            <NSpace align="center" size={6}>
              <NIcon component={DocumentTextOutline} color="#2080f0" />
              <span>{row.name}</span>
            </NSpace>
          )
        }
      },
      {
        title: t('files.type'),
        key: 'type',
        width: 120,
        render(row: Entry) {
          if (row.kind !== 'file') {
            return ''
          }
          const isJar = row.file.fileType === 'jar'
          return h(
            NTag,
            { size: 'small', type: isJar ? 'warning' : 'info' },
            { default: () => (isJar ? t('files.jar') : t('files.file')) }
          )
        }
      },
      {
        title: t('files.size'),
        key: 'size',
        width: 120,
        render(row: Entry) {
          return row.kind === 'file' ? formatSize(row.file.fileSize) : ''
        }
      },
      {
        title: t('files.last_modified'),
        key: 'lastModified',
        width: 200,
        render(row: Entry) {
          return row.kind === 'file' && row.file.lastModified
            ? new Date(row.file.lastModified).toLocaleString()
            : ''
        }
      },
      {
        title: t('files.actions'),
        key: 'actions',
        width: 180,
        render(row: Entry) {
          if (row.kind === 'dir') {
            return (
              <NPopconfirm onPositiveClick={() => removeDir(row)}>
                {{
                  trigger: () => (
                    <NButton size="small" tertiary type="error">
                      {t('files.delete')}
                    </NButton>
                  ),
                  default: () => t('files.delete_dir_confirm', { name: row.name })
                }}
              </NPopconfirm>
            )
          }
          return (
            <NSpace>
              <NButton size="small" tertiary onClick={() => download(row.file)}>
                {t('files.download')}
              </NButton>
              <NPopconfirm onPositiveClick={() => remove(row.file)}>
                {{
                  trigger: () => (
                    <NButton size="small" tertiary type="error">
                      {t('files.delete')}
                    </NButton>
                  ),
                  default: () => t('files.delete_confirm', { name: row.file.fileName })
                }}
              </NPopconfirm>
            </NSpace>
          )
        }
      }
    ]

    const columns = createColumns()

    return () => (
      <NLayout>
        <NLayoutContent>
          <div class="w-full bg-white p-6 border border-gray-100 rounded-xl">
            <div class="flex justify-between items-center pb-4">
              <h2 class="font-bold text-2xl">{t('files.files')}</h2>
              <NSpace>
                <NButton onClick={() => fetch()}>
                  {{
                    icon: () => <NIcon component={RefreshOutline} />,
                    default: () => t('files.refresh')
                  }}
                </NButton>
                <NButton type="primary" onClick={openUpload}>
                  {{
                    icon: () => <NIcon component={CloudUploadOutline} />,
                    default: () => t('files.upload')
                  }}
                </NButton>
              </NSpace>
            </div>
            <NTabs value={typeTab.value} onUpdateValue={onUpdateTab}>
              <NTabPane name="all" tab={t('files.all')} />
              <NTabPane name="file" tab={t('files.file')} />
              <NTabPane name="jar" tab={t('files.jar')} />
            </NTabs>
            <NBreadcrumb class="py-3">
              <NBreadcrumbItem onClick={() => (currentDir.value = '')}>
                {{
                  default: () => [
                    <NIcon component={HomeOutline} style="margin-right: 4px" />,
                    t('files.root')
                  ]
                }}
              </NBreadcrumbItem>
              {segments.value.map((segment, index) => (
                <NBreadcrumbItem
                  onClick={() =>
                    (currentDir.value = segments.value.slice(0, index + 1).join('/'))
                  }
                >
                  {{ default: () => segment }}
                </NBreadcrumbItem>
              ))}
            </NBreadcrumb>
            <NDataTable
              columns={columns}
              data={entries.value}
              loading={loading.value}
              rowKey={(row: Entry) => row.key}
              pagination={false}
              bordered={false}
            />
          </div>
          <NModal
            preset="card"
            title={t('files.upload')}
            show={showModal.value}
            style="width: 520px"
            onUpdateShow={(show: boolean) => (showModal.value = show)}
          >
            <NSpace vertical size={16}>
              <NSpace align="center">
                <span>{t('files.upload_type')}</span>
                <NRadioGroup
                  value={uploadType.value}
                  onUpdateValue={(value: string) => (uploadType.value = value as FileType)}
                >
                  <NRadioButton value="file">{t('files.file')}</NRadioButton>
                  <NRadioButton value="jar">{t('files.jar')}</NRadioButton>
                </NRadioGroup>
              </NSpace>
              <NSpace align="center">
                <span>{t('files.upload_target_dir')}</span>
                <NTag size="small">{currentDir.value || '/'}</NTag>
              </NSpace>
              <NUpload ref={uploadRef} customRequest={customRequest}>
                <NButton>{t('files.drag_or_click')}</NButton>
              </NUpload>
            </NSpace>
          </NModal>
        </NLayoutContent>
      </NLayout>
    )
  }
})
