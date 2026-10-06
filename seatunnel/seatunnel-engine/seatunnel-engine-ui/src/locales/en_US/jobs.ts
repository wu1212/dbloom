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
    runningJobs: 'Running Jobs',
    finishedJobs: 'Finished Jobs',
    view: 'View',
    stop: 'Stop',
    cancel: 'Cancel',
    stopConfirm: 'Stop the job (with savepoint)?',
    cancelConfirm: 'Cancel the job (without savepoint)?',
    stopSuccess: 'Stop request submitted',
    cancelSuccess: 'Cancel request submitted',
    stopFail: 'Failed to stop the job',
    cancelFail: 'Failed to cancel the job',
    createJob: 'Create Job',
    submit: 'Submit',
    submitResult: 'Submission Result',
    submitSuccess: 'Job submitted successfully',
    submitFail: 'Failed to submit the job',
    invalidJson: 'The input is not valid JSON',
    jobConfigPlaceholder:
        'Paste the request body (JSON) of the submit-job REST API, e.g. {"env": {"job.mode": "batch"}, "source": [...], "sink": [...]}'
}
