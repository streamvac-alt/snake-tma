let adminKey = null;
let ordersInterval = null;

window.onload = () => {
    const savedKey = localStorage.getItem('adminKey');
    if (savedKey) {
        fetch(`/api/orders?key=${savedKey}`).then(res => {
            if (res.ok) { adminKey = savedKey; showApp(); }
            else { localStorage.removeItem('adminKey'); }
        });
    }
};

function login() {
    const pwd = document.getElementById('pass-input').value;
    if (!pwd) return;
    fetch(`/api/orders?key=${pwd}`).then(res => {
        if (res.ok) {
            adminKey = pwd;
            localStorage.setItem('adminKey', pwd);
            showApp();
        } else alert('Неверный пароль!');
    });
}

function logout() { localStorage.removeItem('adminKey'); location.reload(); }

function showApp() {
    document.getElementById('login-screen').style.display = 'none';
    document.getElementById('app-screen').style.display = 'block';
    loadShiftStatus(); loadOrders(); loadMenu();
    ordersInterval = setInterval(loadOrders, 5000);
}

function switchTab(tab, btn) {
    document.querySelectorAll('.tabs button').forEach(b => b.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    btn.classList.add('active');
    document.getElementById('tab-' + tab).classList.add('active');
}

async function loadShiftStatus() {
    try {
        const res = await fetch(`/api/shift/status?key=${adminKey}`);
        const data = await res.json();
        const panel = document.querySelector('.shift-panel');
        document.getElementById('shift-status-text').textContent = data.is_open ? 'Открыта' : 'Закрыта';
        document.getElementById('btn-shift').textContent = data.is_open ? 'Закрыть смену' : 'Открыть смену';
        panel.classList.toggle('open', data.is_open);
    } catch(e) { console.error(e); }
}

async function toggleShift() {
    await fetch(`/api/shift?key=${adminKey}`, { method: 'POST' });
    loadShiftStatus();
}

async function loadOrders() {
    if (!adminKey) return;
    const res = await fetch(`/api/orders?key=${adminKey}`);
    if (!res.ok) return;
    const orders = await res.json();
    const list = document.getElementById('orders-list');
    if (orders.length === 0) { list.innerHTML = '<div class="empty">Нет активных заказов</div>'; return; }
    list.innerHTML = orders.sort((a,b) => b.order_number - a.order_number).map(o => `
        <div class="order-card">
            <div class="order-header"><span class="order-num">#${o.order_number}</span><span class="order-user">${o.user_name || 'Гость'}</span></div>
            <div class="order-items">${o.items ? o.items.map(i => `<div class="order-item"><span>${i.product_name}</span><span>${i.qty} шт.</span></div>`).join('') : ''}</div>
            <div class="order-total">${o.total_price} GEL</div>
            <div class="order-actions">
                ${o.status === 'new' ? `<button class="btn-accept" onclick="updateStatus('${o.id}', 'accepted')">Принять</button>` : ''}
                ${o.status === 'accepted' ? `<button class="btn-ready" onclick="updateStatus('${o.id}', 'ready')">Готов</button>` : ''}
                <button class="btn-delete" onclick="deleteOrder('${o.id}')">🗑</button>
            </div>
        </div>
    `).join('');
}

async function loadMenu() {
    try {
        const res = await fetch('/api/menu');
        const data = await res.json();
        const list = document.getElementById('menu-list');
        let html = '';
        data.categories.forEach(cat => {
            const catProducts = data.products.filter(p => p.category_id === cat.id);
            if (catProducts.length === 0) return;
            html += `<h3 style="margin: 20px 0 10px; color: var(--yellow);">${cat.name}</h3>`;
            catProducts.forEach(p => {
                p.variants.forEach(v => {
                    html += `
                    <div class="menu-item">
                        <div class="menu-info"><h4>${p.name}</h4><p>${v.name} — ${v.price} GEL</p></div>
                        <button class="toggle-stop" onclick="toggleStop(${v.id}, this)"></button>
                    </div>`;
                });
            });
        });
        list.innerHTML = html || '<div class="empty">Меню пустое</div>';
    } catch(e) { console.error(e); }
}

async function updateStatus(id, status) {
    await fetch(`/api/order/status?key=${adminKey}`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id, status })
    });
    loadOrders();
}

async function deleteOrder(id) {
    if (!confirm('Удалить заказ?')) return;
    await fetch(`/api/order/delete?key=${adminKey}`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id })
    });
    loadOrders();
}

async function toggleStop(variantId, btn) {
    btn.classList.toggle('stopped');
    const isStopped = btn.classList.contains('stopped');
    await fetch(`/api/variant/stop?key=${adminKey}`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ variant_id: variantId, is_stopped: isStopped })
    });
}
