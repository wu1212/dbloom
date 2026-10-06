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

import { get } from '@/service/service'
import type { JobLog } from './types'

export const getJobLogs = (jobId: string) => get<JobLog[]>(`/logs/${jobId}?format=json`)
export const getJobLogContent = (logName: string) => get<JobLog[]>(`/log/${logName}`)

/**
 * Stream the log content from the log proxy link. The onChunk callback receives
 * decoded text chunks and may return false to stop reading (e.g. size limit reached).
 */
export const streamJobLog = async (
  logLink: string,
  options: { onChunk: (text: string) => boolean | void; signal?: AbortSignal }
) => {
  const response = await fetch(logLink, { signal: options.signal })
  if (!response.ok || response.body === null) {
    throw new Error(`Failed to fetch log, status: ${response.status}`)
  }
  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  for (;;) {
    const { done, value } = await reader.read()
    if (done) {
      break
    }
    const keep = options.onChunk(decoder.decode(value, { stream: true }))
    if (keep === false) {
      await reader.cancel()
      break
    }
  }
}

export const JobLogService = {
  getJobLogs,
  getJobLogContent,
  streamJobLog
}
