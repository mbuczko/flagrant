{{/*
Expand the name of the chart.
*/}}
{{- define "flagrant.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "flagrant.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Common labels.
*/}}
{{- define "flagrant.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{ include "flagrant.selectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels.
*/}}
{{- define "flagrant.selectorLabels" -}}
app.kubernetes.io/name: {{ include "flagrant.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Name of the service account to use.
*/}}
{{- define "flagrant.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "flagrant.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Name of the Secret holding flagrant.toml.
*/}}
{{- define "flagrant.configSecretName" -}}
{{- .Values.config.existingSecret | default (printf "%s-config" (include "flagrant.fullname" .)) }}
{{- end }}

{{/*
Name of the ConfigMap holding litestream.yml.
*/}}
{{- define "flagrant.litestreamConfigMapName" -}}
{{- printf "%s-litestream" (include "flagrant.fullname" .) }}
{{- end }}

{{/*
Name of the Secret holding S3 credentials for Litestream, if the chart renders one at all.
*/}}
{{- define "flagrant.litestreamSecretName" -}}
{{- printf "%s-litestream-s3" (include "flagrant.fullname" .) }}
{{- end }}

{{/*
Whether any static S3 credentials were supplied (existing or inline).
*/}}
{{- define "flagrant.hasLitestreamCredentials" -}}
{{- if or .Values.litestream.s3.existingSecret (and .Values.litestream.s3.accessKeyId .Values.litestream.s3.secretAccessKey) }}true{{- end }}
{{- end }}

{{/*
Env vars injecting S3 credentials into litestream containers, if any static
credentials were supplied at all. When neither existingSecret nor inline
accessKeyId/secretAccessKey are set, this renders nothing, leaving credential
resolution to the pod's ServiceAccount (e.g. AWS IRSA).
*/}}
{{- define "flagrant.litestreamEnv" -}}
{{- if include "flagrant.hasLitestreamCredentials" . }}
{{- $secretName := .Values.litestream.s3.existingSecret | default (include "flagrant.litestreamSecretName" .) }}
- name: LITESTREAM_ACCESS_KEY_ID
  valueFrom:
    secretKeyRef:
      name: {{ $secretName }}
      key: {{ .Values.litestream.s3.existingSecretAccessKeyIdKey }}
- name: LITESTREAM_SECRET_ACCESS_KEY
  valueFrom:
    secretKeyRef:
      name: {{ $secretName }}
      key: {{ .Values.litestream.s3.existingSecretSecretAccessKeyKey }}
{{- end }}
{{- end }}
