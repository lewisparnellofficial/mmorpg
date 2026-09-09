#include <QGuiApplication>
#include <QElapsedTimer>
#include <QFocusEvent>
#include <QMouseEvent>
#include <QPointingDevice>
#include <QProcess>
#include <QQmlContext>
#include <QQuickView>
#include <QTabletEvent>
#include <QUrl>

class TabletBridgeWindow final : public QQuickView {
    Q_OBJECT
    Q_PROPERTY(QString lastInput READ lastInput NOTIFY lastInputChanged)
    Q_PROPERTY(QString coreStatus READ coreStatus NOTIFY coreStatusChanged)

public:
    explicit TabletBridgeWindow(QWindow* parent = nullptr)
        : QQuickView(parent)
    {
        clock_.start();
        setResizeMode(QQuickView::SizeRootObjectToView);
        rootContext()->setContextProperty(QStringLiteral("tabletBridge"), this);
        startCoreBridge();
    }

    ~TabletBridgeWindow() override
    {
        if (core_.state() == QProcess::Running) {
            sendCommand(QStringLiteral("quit"));
            core_.waitForFinished(500);
        }
    }

    QString lastInput() const { return lastInput_; }
    QString coreStatus() const { return coreStatus_; }

    Q_INVOKABLE void selectBrush(const QString& operation)
    {
        sendCommand(QStringLiteral("brush %1").arg(operation));
    }

    Q_INVOKABLE void saveTerrain()
    {
        sendCommand(QStringLiteral("save %1").arg(terrainPath()));
    }

    Q_INVOKABLE void reloadTerrain()
    {
        sendCommand(QStringLiteral("open %1").arg(terrainPath()));
    }

    Q_INVOKABLE void captureStroke()
    {
        sendCommand(QStringLiteral("capture %1").arg(capturePath()));
    }

    Q_INVOKABLE void undoTerrain() { sendCommand(QStringLiteral("undo")); }
    Q_INVOKABLE void redoTerrain() { sendCommand(QStringLiteral("redo")); }

signals:
    void lastInputChanged();
    void coreStatusChanged();
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
            sendEvent(QStringLiteral("proximity-enter"), 0.0, 0.0, 0.0, source, 0.0, 0.0,
                      0.0);
        } else if (phase == QEvent::TabletLeaveProximity) {
            cancelStroke(QStringLiteral("proximity lost"));
            setLastInput(QStringLiteral("%1 proximity-leave").arg(source));
            sendEvent(QStringLiteral("proximity-leave"), 0.0, 0.0, 0.0, source, 0.0, 0.0,
                      0.0);
        } else if (phase == QEvent::TabletPress) {
            tabletStrokeActive_ = true;
            suppressMouseUntilRelease_ = true;
            emitSample(QStringLiteral("press"), point.x(), point.y(), pressure, source,
                       event->xTilt(), event->yTilt(), event->rotation());
        } else if (phase == QEvent::TabletMove && tabletStrokeActive_) {
            emitSample(QStringLiteral("move"), point.x(), point.y(), pressure, source,
                       event->xTilt(), event->yTilt(), event->rotation());
        } else if (phase == QEvent::TabletRelease && tabletStrokeActive_) {
            emitSample(QStringLiteral("release"), point.x(), point.y(), pressure, source,
                       event->xTilt(), event->yTilt(), event->rotation());
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
                   QStringLiteral("mouse"), 0.0, 0.0, 0.0);
        event->accept();
    }

    void mouseMoveEvent(QMouseEvent* event) override
    {
        if (mouseStrokeActive_ && !suppressMouseUntilRelease_) {
            emitSample(QStringLiteral("move"), event->position().x(), event->position().y(), 1.0,
                       QStringLiteral("mouse"), 0.0, 0.0, 0.0);
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
                       1.0, QStringLiteral("mouse"), 0.0, 0.0, 0.0);
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
        sendEvent(QStringLiteral("cancel"), 0.0, 0.0, 0.0, QStringLiteral("pen"), 0.0, 0.0,
                  0.0);
        emit strokeCancelled(reason);
    }

    void emitSample(const QString& phase, double x, double y, double pressure,
                    const QString& source, double tiltX, double tiltY, double rotation)
    {
        const auto timestamp = clock_.nsecsElapsed();
        setLastInput(QStringLiteral("%1 %2 x=%3 y=%4 pressure=%5 timestamp_ns=%6")
                         .arg(source, phase)
                         .arg(x, 0, 'f', 1)
                         .arg(y, 0, 'f', 1)
                         .arg(pressure, 0, 'f', 3)
                         .arg(timestamp));
        sendEvent(phase, x, y, pressure, source, tiltX, tiltY, rotation, timestamp);
        if (phase == QStringLiteral("press")) {
            emit strokeStarted(x, y, pressure, source);
        } else if (phase == QStringLiteral("move")) {
            emit strokePoint(x, y, pressure, source);
        }
    }

    void startCoreBridge()
    {
        const auto program = qEnvironmentVariable("MMORPG_EDITOR_CORE_BRIDGE",
                                                   QStringLiteral("target/debug/mmorpg-editor-core"));
        connect(&core_, &QProcess::readyReadStandardOutput, this, [this]() {
            const auto output = QString::fromUtf8(core_.readAllStandardOutput()).trimmed();
            if (!output.isEmpty()) {
                coreStatus_ = output.split('\n').constLast();
                emit coreStatusChanged();
            }
        });
        connect(&core_, &QProcess::errorOccurred, this, [this](QProcess::ProcessError) {
            coreStatus_ = core_.errorString();
            emit coreStatusChanged();
        });
        core_.start(program, {QStringLiteral("--bridge")});
    }

    void sendEvent(const QString& phase, double x, double y, double pressure,
                   const QString& source, double tiltX, double tiltY, double rotation,
                   qint64 timestamp = -1)
    {
        if (timestamp < 0) {
            timestamp = clock_.nsecsElapsed();
        }
        sendCommand(QStringLiteral("event %1 %2 %3 %4 %5 %6 %7 %8 %9")
                        .arg(phase)
                        .arg(x, 0, 'f', 4)
                        .arg(y, 0, 'f', 4)
                        .arg(pressure, 0, 'f', 4)
                        .arg(source)
                        .arg(tiltX, 0, 'f', 4)
                        .arg(tiltY, 0, 'f', 4)
                        .arg(rotation, 0, 'f', 4)
                        .arg(timestamp));
    }

    void sendCommand(const QString& command)
    {
        if (core_.state() != QProcess::Running) {
            return;
        }
        core_.write(command.toUtf8());
        core_.write("\n");
        core_.waitForBytesWritten(100);
    }

    QString terrainPath() const
    {
        return qEnvironmentVariable("MMORPG_EDITOR_TERRAIN_OUTPUT",
                                    QStringLiteral("/tmp/mmorpg-editor-qt-terrain.mmterrain"));
    }

    QString capturePath() const
    {
        return qEnvironmentVariable("MMORPG_EDITOR_CAPTURE_OUTPUT",
                                    QStringLiteral("/tmp/mmorpg-editor-qt-stroke.mmstroke"));
    }

    QString lastInput_ = QStringLiteral("waiting for tablet or mouse input");
    bool tabletStrokeActive_ = false;
    bool mouseStrokeActive_ = false;
    bool suppressMouseUntilRelease_ = false;
    QElapsedTimer clock_;
    QProcess core_;
    QString coreStatus_ = QStringLiteral("starting Rust editor core bridge");
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
