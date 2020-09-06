{
  const d = domlib, df = domlib.domfn 

  const authLauncher = d.query('.auth-launcher')

  const {authView, usernameInput, passwordInput} = d.h`
<section class="auth-view" ref="authView">
  <div>
    <div class="auth-form-field">
      <label for="auth-username">Username</label>
      <input type="text" name="username" id="auth-username" ref="usernameInput">
      <label for="auth-pwd">Password</label>
      <input type="password" name="password" id="auth-pwd" ref="passwordInput">
    </div>
    <button class="submit" onclick="() => app.authenticate()">authenticate</button>
  </div>
</section>`.collect()

  app.authView = authView

  app.authViewToggle = (state = !df.hasClass(authView, 'open')) => {
    df.class(authView,'open', state)
    df.class(authLauncher,'active', state)
    if (state) {
      d.render(authView)
      clickOutHandler.on()
    } else {
      df.remove(authView)
    }
    return state
  }

  const clickOutHandler = d.on.click(document.body, e => {
    if (
      e.target != authView &&
      !authView.contains(e.target) &&
      df.hasClass(authView, 'open') &&
      e.target != authLauncher
    ) {
      e.preventDefault()
      app.authViewToggle(false)
      clickOutHandler.off()
    }
  }).off()

  d.on.click(authLauncher, app.clickAuthLauncher = e => {
    e.preventDefault()
    app.authViewToggle()
  })

  app.authenticate = (
    username = usernameInput.value.trim(),
    password = passwordInput.value.trim()
  ) => {
    if (username == '') throw new Error('username is invalid')
    if (password == '') throw new Error('password is invalid')

    app.jsonPost('/auth', { username, password })
    .then(res => res.json())
    .then(data => {
      console.log(data)
      if (data.ok) location.reload()
    })
  }
}
