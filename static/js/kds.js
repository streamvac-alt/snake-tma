let currentDept = 'bar';
let pin = '';
let adminKey = '';
let ordersInterval = null;

function setDept(dept) {
    currentDept = dept;
    document.getElementById('tab-bar').classList.remove('active');
    document.getElementById('tab-kitchen').classList.remove('active');
    document.getElementById('tab-' + dept).classList.add('active');
}

function login() {
    pin = document.getElementById('pin-input').value;
    if (pin === '1111') { adminKey = 'boss123'; } 
    else if (pin === '2222') { adminKey = 'boss123'; } 
    else { return alert('Неверный пин-код'); }

    document.getElementById('login-screen').style.display = 'none';
    document.getElementById('app-screen').style.display = 'flex';
    document.getElementById('current-dept').textContent = currentDept === 'bar' ? 'Бар' : 'Кухня';

    loadOrders();
    ordersInterval = setInterval(loadOrders, 3000);
    checkConnection();
}

function logout() {
    clearInterval(ordersInterval);
    location.reload();
}

async function loadOrders() {
    try {
        const res = await fetch(`/api/orders?key=${adminKey}`);
        if (!res.ok) throw new Error('Offline');
        
        const orders = await res.json();
        
        document.querySelector('.dot').className = 'dot online';
        document.getElementById('conn-status').textContent = 'Онлайн';

        const activeOrders = orders.filter(o => o.status !== 'done' && o.status !== 'ready').sort((a,b) => b.id.localeCompare(a.id));
        
        const container = document.getElementById('orders-grid');
        if (activeOrders.length === 0) {
            container.innerHTML = '<div class="empty-state">Нет активных заказов</div>';
            return;
        }

        const now = Date.now();
        container.innerHTML = activeOrders.map(order => {
            const elapsed = Math.floor((now - new Date(order.created_at).getTime()) / 1000);
            const minutes = Math.floor(elapsed / 60);
            const seconds = elapsed % 60;
            const timeStr = `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
            
            let timerClass = '';
            if (minutes >= 10) timerClass = 'danger';
            else if (minutes >= 5) timerClass = 'warning';

            const isAccepted = order.status === 'accepted';
            const itemsHtml = order.items.map(i => `
                <div class="order-item">
                    <span class="order-item-name">${i.product_name} (${i.variant_name})</span>
                    <span class="order-item-qty">x${i.qty}</span>
                </div>
            `).join('');
            
            const commentHtml = order.comment ? `<div class="order-comment">${order.comment}</div>` : '';
            const actionBtn = isAccepted 
                ? `<button class="btn-action btn-ready" onclick="updateStatus('${order.id}', 'ready')">Отдать</button>`
                : `<button class="btn-action btn-accept" onclick="updateStatus('${order.id}', 'accepted')">Принять</button>`;

            return `
            <div class="order-card ${isAccepted ? 'accepted' : ''}">
                <div class="order-header">
                    <div class="order-num">#${order.order_number}</div>
                    <div class="order-timer ${timerClass}">${timeStr}</div>
                </div>
                <div class="order-items">${itemsHtml}</div>
                ${commentHtml}
                ${actionBtn}
            </div>`;
        }).join('');
    } catch (e) {
        document.querySelector('.dot').className = 'dot offline';
        document.getElementById('conn-status').textContent = 'Оффлайн';
    }
}

async function updateStatus(id, status) {
    await fetch(`/api/order/status?key=${adminKey}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id, status })
    });
    loadOrders();
}

function checkConnection() {
    if (!navigator.onLine) {
        document.querySelector('.dot').className = 'dot offline';
        document.getElementById('conn-status').textContent = 'Оффлайн';
    }
}
window.addEventListener('online', checkConnection);
window.addEventListener('offline', checkConnection);
