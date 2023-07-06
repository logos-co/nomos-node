def sendMessage(Map args=[:]) {
  def opts = [
    header: args.header ?: 'Nightly Fuzztest Passed',
    title:  args.title  ?: "${env.JOB_NAME}#${env.BUILD_NUMBER}",
    cred:   args.cred   ?: null,
  ]
  def repo = [
    url: GIT_URL.minus('.git'),
    branch: GIT_BRANCH.minus('origin/'),
    commit: GIT_COMMIT.take(8),
  ]
  withCredentials([
    string(
      credentialsId: opts.cred,
      variable: 'DISCORD_WEBHOOK',
    ),
  ]) {
    discordSend(
      link: env.BUILD_URL,
      result: currentBuild.currentResult,
      webhookURL: env.DISCORD_WEBHOOK,
      title: opts.title,
      description: """
      ${opts.header}
      Branch: [`${repo.branch}`](${repo.url}/commits/${repo.branch})
      Commit: [`${repo.commit}`](${repo.url}/commit/${repo.commit})
      """,
    )
  }
}

return this
