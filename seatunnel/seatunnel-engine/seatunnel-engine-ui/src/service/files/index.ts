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

import { get, axios } from '@/service/service'
import type { FileType, UploadedFile, FileExistsResult, FileDeleteResult } from './types'

const baseURL: string = import.meta.env.VITE_APP_API_BASE || ''

export const listFiles = () => get<UploadedFile[]>('/upload-file')

export const existsFile = (fileName: string, fileType: FileType) =>
  get<FileExistsResult>('/file-exists', { fileName, fileType })

export const deleteFile = (fileName: string, fileType: FileType, isDir = false) =>
  axios.delete<FileDeleteResult>('/upload-file', {
    params: { fileName, fileType, ...(isDir ? { isDir: true } : {}) }
  })

export const uploadFile = (
  file: File,
  fileType: FileType,
  relativePath?: string,
  onUploadProgress?: (progressEvent: any) => void
) => {
  const formData = new FormData()
  formData.append('file', file)
  return axios.post('/upload-file', formData, {
    params: { fileType, ...(relativePath ? { relativePath } : {}) },
    timeout: 0,
    onUploadProgress
  })
}

export const buildDownloadUrl = (fileName: string, fileType: FileType) =>
  `${baseURL}/download-file?fileType=${encodeURIComponent(
    fileType
  )}&fileName=${encodeURIComponent(fileName)}`

export const filesService = {
  listFiles,
  existsFile,
  deleteFile,
  uploadFile,
  buildDownloadUrl
}
