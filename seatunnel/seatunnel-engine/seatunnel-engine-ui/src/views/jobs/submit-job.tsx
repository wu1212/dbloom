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

import { defineComponent, ref } from 'vue'
import { NModal, NInput, NButton, NSpace, NAlert, useMessage } from 'naive-ui'
import { useI18n } from 'vue-i18n'
import { JobsService } from '@/service/job'

interface SubmitResult {
  success: boolean
  content: string
}

export default defineComponent({
  name: 'SubmitJob',
  props: {
    show: {
      type: Boolean,
      default: false
    }
  },
  emits: ['update:show'],
  setup(props, { emit }) {
    const { t } = useI18n()
    const message = useMessage()

    const configContent = ref('')
    const submitting = ref(false)
    const result = ref<SubmitResult | null>(null)

    const close = () => {
      emit('update:show', false)
    }

    const parseConfig = (): Record<string, any> | null => {
      try {
        const config = JSON.parse(configContent.value)
        if (typeof config !== 'object' || config === null || Array.isArray(config)) {
          return null
        }
        return config
      } catch (e) {
        return null
      }
    }

    const submit = async () => {
      result.value = null
      const config = parseConfig()
      if (config === null) {
        result.value = { success: false, content: t('jobs.invalidJson') }
        return
      }
      submitting.value = true
      try {
        const res = await JobsService.submitJob(config)
        result.value = { success: true, content: JSON.stringify(res, null, 2) }
        message.success(t('jobs.submitSuccess'))
      } catch (e: any) {
        const data = e?.response?.data
        const detail =
          typeof data === 'string' ? data : data ? JSON.stringify(data, null, 2) : e?.message
        result.value = { success: false, content: detail || String(e) }
        message.error(t('jobs.submitFail'))
      } finally {
        submitting.value = false
      }
    }

    return () => (
      <NModal
        show={props.show}
        preset="card"
        title={t('jobs.createJob')}
        style={{ width: '720px', maxWidth: '90vw' }}
        maskClosable={false}
        onUpdateShow={(show: boolean) => emit('update:show', show)}
      >
        <NSpace vertical size={16}>
          <NInput
            type="textarea"
            value={configContent.value}
            onUpdateValue={(value: string) => (configContent.value = value)}
            placeholder={t('jobs.jobConfigPlaceholder')}
            rows={16}
            style={{ fontFamily: 'monospace' }}
          />
          {result.value && (
            <NAlert
              type={result.value.success ? 'success' : 'error'}
              title={t('jobs.submitResult')}
              showIcon
            >
              <pre class="whitespace-pre-wrap break-all text-sm">{result.value.content}</pre>
            </NAlert>
          )}
          <NSpace justify="end">
            <NButton onClick={close}>{t('jobs.cancel')}</NButton>
            <NButton type="primary" loading={submitting.value} onClick={submit}>
              {t('jobs.submit')}
            </NButton>
          </NSpace>
        </NSpace>
      </NModal>
    )
  }
})
