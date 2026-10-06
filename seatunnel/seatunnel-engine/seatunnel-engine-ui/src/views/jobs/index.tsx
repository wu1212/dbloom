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
import { NSpace, NLayout, NLayoutContent, NButton } from 'naive-ui'
import { useI18n } from 'vue-i18n'
import RunningJobs from '@/views/jobs/running-jobs'
import FinishedJobs from '@/views/jobs/finished-jobs'
import SubmitJob from '@/views/jobs/submit-job'

export default defineComponent({
  setup() {
    const { t } = useI18n()
    const showSubmitJob = ref(false)

    return () => (
      <NLayout>
        <NLayoutContent>
          <NSpace justify="end" class="mb-4">
            <NButton type="primary" onClick={() => (showSubmitJob.value = true)}>
              {t('jobs.createJob')}
            </NButton>
          </NSpace>
          <SubmitJob
            show={showSubmitJob.value}
            {...{ 'onUpdate:show': (show: boolean) => (showSubmitJob.value = show) }}
          />
          <RunningJobs class="mb-6" />
          <FinishedJobs />
        </NLayoutContent>
      </NLayout>
    )
  }
})
