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

import { computed, defineComponent, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { NAlert, NButton, NInput, NSpace, NVirtualList } from 'naive-ui'
import { useI18n } from 'vue-i18n'
import { streamJobLog } from '@/service/job-log'

// Fixed row metrics for the virtual list; lines are rendered without wrapping,
// long lines widen the row and the list shares a single horizontal scrollbar.
const LINE_HEIGHT = 20

export default defineComponent({
  name: 'LogViewer',
  props: {
    logLink: {
      type: String,
      required: true
    },
    logName: {
      type: String,
      required: true
    }
  },
  setup(props) {
    const { t } = useI18n()

    const loading = ref(false)
    const error = ref('')
    const keyword = ref('')

    // Lines are accumulated incrementally as chunks arrive: the stream is never
    // re-split as a whole, so loading stays linear no matter how large the log is.
    // rawLines holds the complete lines (the last element may still be in
    // progress while streaming); items is the key-bearing view required by
    // NVirtualList, kept in sync with rawLines.
    const rawLines: string[] = []
    const items = reactive<Array<{ key: number }>>([])
    const rev = ref(0)

    const virtualListRef = ref<InstanceType<typeof NVirtualList> | null>(null)
    // Keep auto-scrolling only while the user stays near the bottom.
    const stickToBottom = ref(true)

    let abortController: AbortController | null = null

    const syncItems = () => {
      for (let i = items.length; i < rawLines.length; i++) {
        items.push({ key: i })
      }
      if (items.length > rawLines.length) {
        items.splice(rawLines.length)
      }
      // Replace the last item so that the in-progress tail line re-renders even
      // when the latest chunk did not contain a line break.
      if (items.length > 0) {
        items[items.length - 1] = { key: rawLines.length - 1 }
      }
    }

    const appendChunk = (text: string) => {
      // Merge the chunk with the in-progress tail line and split again, so that
      // lines cut by chunk boundaries are reassembled.
      const tail = rawLines.length > 0 ? (rawLines.pop() as string) : ''
      rawLines.push(...(tail + text).split('\n'))
      syncItems()
      rev.value++
    }

    const fetchLog = async () => {
      abortController?.abort()
      const controller = new AbortController()
      abortController = controller
      rawLines.length = 0
      items.splice(0)
      rev.value++
      error.value = ''
      loading.value = true
      stickToBottom.value = true
      try {
        await streamJobLog(props.logLink, {
          signal: controller.signal,
          onChunk: appendChunk
        })
      } catch (e: any) {
        if (e?.name !== 'AbortError') {
          error.value = t('detail.logs.fetchFailed')
        }
      } finally {
        loading.value = false
      }
    }

    onMounted(fetchLog)
    onUnmounted(() => abortController?.abort())

    const viewData = computed(() => {
      // Reading rev makes this computed re-evaluate as the stream grows.
      void rev.value
      const kw = keyword.value.trim().toLowerCase()
      if (kw === '') {
        return { lines: rawLines, items }
      }
      const lines: string[] = []
      const matched: Array<{ key: number }> = []
      rawLines.forEach((line, index) => {
        if (line.toLowerCase().includes(kw)) {
          lines.push(line)
          matched.push({ key: index })
        }
      })
      return { lines, items: matched }
    })

    const onVlScroll = (e: Event) => {
      const el = e.target as HTMLElement
      stickToBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight < 50
    }

    // While streaming, follow the tail; once loading finishes, jump to the last line.
    watch(viewData, () => {
      if (!stickToBottom.value) {
        return
      }
      const count = viewData.value.lines.length
      if (count === 0) {
        return
      }
      nextTick(() => virtualListRef.value?.scrollTo({ index: count - 1, debounce: false }))
    })

    const renderLine = ({ item, index }: { item: { key: number }; index: number }) => (
      <div
        key={item.key}
        style={{
          height: `${LINE_HEIGHT}px`,
          lineHeight: `${LINE_HEIGHT}px`,
          padding: '0 12px',
          whiteSpace: 'pre',
          // Size the row to its content so that the whole list shares one
          // horizontal scrollbar, instead of a scrollbar per line.
          width: 'fit-content',
          minWidth: '100%',
          boxSizing: 'border-box'
        }}
      >
        {viewData.value.lines[index]}
      </div>
    )

    const download = () => {
      const blob = new Blob([viewData.value.lines.join('\n')], {
        type: 'text/plain;charset=utf-8'
      })
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = props.logName
      anchor.click()
      URL.revokeObjectURL(url)
    }

    return () => (
      <NSpace vertical size={12}>
        <NSpace align="center" justify="space-between">
          <NSpace align="center" size={12}>
            <NInput
              value={keyword.value}
              onUpdateValue={(value: string) => (keyword.value = value)}
              placeholder={t('detail.logs.keywordPlaceholder')}
              clearable
              style={{ width: '280px' }}
            />
            <span class="text-sm text-gray-500">
              {keyword.value.trim() === ''
                ? t('detail.logs.totalLines', { count: viewData.value.lines.length })
                : t('detail.logs.matchedLines', { count: viewData.value.lines.length })}
            </span>
          </NSpace>
          <NSpace>
            <NButton size="small" onClick={download} disabled={rawLines.length === 0}>
              {t('detail.logs.download')}
            </NButton>
            <NButton size="small" onClick={fetchLog} loading={loading.value}>
              {t('detail.logs.refresh')}
            </NButton>
          </NSpace>
        </NSpace>
        {error.value !== '' && <NAlert type="error">{error.value}</NAlert>}
        <NVirtualList
          ref={virtualListRef}
          items={viewData.value.items}
          itemSize={LINE_HEIGHT}
          style={{
            height: '560px',
            background: '#fafafa',
            borderRadius: '6px',
            fontFamily: 'monospace',
            fontSize: '12px'
          }}
          onScroll={onVlScroll}
          v-slots={{ default: renderLine }}
        />
      </NSpace>
    )
  }
})
