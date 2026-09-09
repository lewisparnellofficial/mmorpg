#include <QGuiApplication>
#include <QElapsedTimer>
#include <QFocusEvent>
#include <QMouseEvent>
#include <QPointingDevice>
#include <QQmlContext>
#include <QQuickView>
#include <QTabletEvent>
#include <QUrl>

class TabletBridgeWindow final : public QQuickView {
    Q_OBJECT
    Q_PROPERTY(QString lastInput READ lastInput NOTIFY lastInputChanged)

public:
    explicit TabletBridgeWindow(QWindow* parent = nullptr)
        : QQuickView(parent)
    {
        clock_.start();
        setResizeMode(QQuickView::SizeRootObjectToView);
        rootContext()->setContextProperty(QStringLiteral("tabletBridge"), this);
    }

    QString lastInput() const { return lastInput_; }

signals:
    void lastInputChanged();
    void strokeStarted(double x, double y, double pressure, QString source);
    void strokePoint(double x, double y, double pressure, QString source);
    void strokeFinished(QString source);
    void strokeCancelled(QString reason);

protected:
    void tabletEvent(QTabletEvent* event) override
    {
        const auto phase = event->type();
        const auto point = event->position();
        const auto pressure = event->pressure();
        const auto source = event->pointerType() == QPointingDevice::PointerType::Eraser
            ? QStringLiteral("eraser")
            : QStringLiteral("pen");

        if (phase == QEvent::TabletEnterProximity) {
            setLastInput(QStringLiteral("%1 proximity-enter").arg(source));
        } else if (phase == QEvent::TabletLeaveProximity) {
            cancelStroke(QStringLiteral("proximity lost"));
            setLastInput(QStringLiteral("%1 proximity-leave").arg(source));
        } else if (phase == QEvent::TabletPress) {
            tabletStrokeActive_ = true;
            suppressMouseUntilRelease_ = true;
            emitSample(QStringLiteral("press"), point.x(), point.y(), pressure, source);
        } else if (phase == QEvent::TabletMove && tabletStrokeActive_) {
            emitSample(QStringLiteral("move"), point.x(), point.y(), pressure, source);
        } else if (phase == QEvent::TabletRelease && tabletStrokeActive_) {
            emitSample(QStringLiteral("release"), point.x(), point.y(), pressure, source);
            tabletStrokeActive_ = false;
            suppressMouseUntilRelease_ = false;
            emit strokeFinished(source);
        }

        event->accept();
    }

    void focusOutEvent(QFocusEvent* event) override
    {
        cancelStroke(QStringLiteral("focus lost"));
        QQuickView::focusOutEvent(event);
    }

    void mousePressEvent(QMouseEvent* event) override
    {
        if (suppressMouseUntilRelease_) {
            event->accept();
            return;
        }
        mouseStrokeActive_ = true;
        emitSample(QStringLiteral("press"), event->position().x(), event->position().y(), 1.0,
                   QStringLiteral("mouse"));
        event->accept();
    }

    void mouseMoveEvent(QMouseEvent* event) override
    {
        if (mouseStrokeActive_ && !suppressMouseUntilRelease_) {
            emitSample(QStringLiteral("move"), event->position().x(), event->position().y(), 1.0,
                       QStringLiteral("mouse"));
        }
        event->accept();
    }

    void mouseReleaseEvent(QMouseEvent* event) override
    {
        if (suppressMouseUntilRelease_) {
            event->accept();
            return;
        }
        if (mouseStrokeActive_) {
            emitSample(QStringLiteral("release"), event->position().x(), event->position().y(),
                       1.0, QStringLiteral("mouse"));
            mouseStrokeActive_ = false;
            emit strokeFinished(QStringLiteral("mouse"));
        }
        event->accept();
    }

private:
    void setLastInput(const QString& value)
    {
        lastInput_ = value;
        emit lastInputChanged();
    }

    void cancelStroke(const QString& reason)
    {
        if (!tabletStrokeActive_ && !mouseStrokeActive_) {
            return;
        }
        tabletStrokeActive_ = false;
        mouseStrokeActive_ = false;
        suppressMouseUntilRelease_ = false;
        emit strokeCancelled(reason);
    }

    void emitSample(const QString& phase, double x, double y, double pressure,
                    const QString& source)
    {
        setLastInput(QStringLiteral("%1 %2 x=%3 y=%4 pressure=%5 timestamp_ns=%6")
                         .arg(source, phase)
                         .arg(x, 0, 'f', 1)
                         .arg(y, 0, 'f', 1)
                         .arg(pressure, 0, 'f', 3)
                         .arg(clock_.nsecsElapsed()));
        if (phase == QStringLiteral("press")) {
            emit strokeStarted(x, y, pressure, source);
        } else if (phase == QStringLiteral("move")) {
            emit strokePoint(x, y, pressure, source);
        }
    }

    QString lastInput_ = QStringLiteral("waiting for tablet or mouse input");
    bool tabletStrokeActive_ = false;
    bool mouseStrokeActive_ = false;
    bool suppressMouseUntilRelease_ = false;
    QElapsedTimer clock_;
};

int main(int argc, char** argv)
{
    QGuiApplication application(argc, argv);
    TabletBridgeWindow window;
    window.setTitle(QStringLiteral("MMORPG Terrain Editor — Qt tablet proof"));
    window.setSource(QUrl(QStringLiteral("qrc:/qt/qml/Mmorpg/Editor/Main.qml")));
    window.resize(1280, 800);
    window.show();
    return application.exec();
}

#include "main.moc"
