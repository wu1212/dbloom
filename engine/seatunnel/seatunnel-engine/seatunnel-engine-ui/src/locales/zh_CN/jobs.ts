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

export default {
    runningJobs: '运行中',
    finishedJobs: '已结束',
    view: '查看',
    stop: '停止',
    cancel: '取消',
    stopConfirm: '停止任务（保存检查点）？',
    cancelConfirm: '取消任务（不保存检查点）？',
    stopSuccess: '停止任务请求已提交',
    cancelSuccess: '取消任务请求已提交',
    stopFail: '停止任务失败',
    cancelFail: '取消任务失败',
    createJob: '创建任务',
    submit: '提交',
    submitResult: '提交结果',
    submitSuccess: '任务提交成功',
    submitFail: '任务提交失败',
    invalidJson: '输入内容不是合法的 JSON',
    jobConfigPlaceholder:
        '请粘贴 REST API 提交任务接口的请求 Body（JSON 格式），例如：{"env": {"job.mode": "batch"}, "source": [...], "sink": [...]}'
}
