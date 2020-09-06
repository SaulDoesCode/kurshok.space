const version = 'v0.0.1'
const cacheName = version + '-grimstack.io'

const cachedPaths = [
  '/css/marx.min.css',
  '/js/domlib.js',
  /* 
  '/',
  '/css/style.css',
  '/js/admin.js',
  '/js/main.js',
  '/js/auth.js',
  '/js/view.js', */
]

self.addEventListener('install', e => {
  e.waitUntil(caches.open(cacheName)
    .then(cache => cache.addAll(cachedPaths))
  )
})

const unableToResolve = () => new Response('<h1>Service Unavailable</h1>', {
  status: 503,
  statusText: 'Service Unavailable',
  headers: new Headers({
    'Content-Type': 'text/html'
  })
})

self.addEventListener('fetch', e => {
  if (cachedPaths.some(path => e.request.url.includes(path))) {
    e.respondWith(caches.match(e.request)
      .then(res => res || fetch(e.request))
    )
  }
})

self.addEventListener("activate", event => {
  event.waitUntil(
    caches.keys()
    .then(keys => Promise.all(keys
      .filter(key => !key.startsWith(version))
      .map(key => caches.delete(key))
    ))
  )
})