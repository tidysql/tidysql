import { Component, type ErrorInfo, type ReactNode } from 'react'

type ErrorBoundaryProps = {
  children: ReactNode
}

type ErrorBoundaryState = {
  hasError: boolean
  errorMessage: string
}

class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = {
    hasError: false,
    errorMessage: '',
  }

  static getDerivedStateFromError(error: Error) {
    return {
      hasError: true,
      errorMessage: error.message,
    }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Unhandled app error', error, info)
  }

  render() {
    if (this.state.hasError) {
      const showDetails = import.meta.env.DEV && this.state.errorMessage
      return (
        <div className="error-screen" role="alert">
          <div className="error-card">
            <h1 className="error-title">Something went wrong.</h1>
            <p className="error-message">
              The editor failed to load. Please refresh and try again.
            </p>
            {showDetails ? (
              <pre className="error-details">{this.state.errorMessage}</pre>
            ) : null}
            <button
              className="btn-primary"
              type="button"
              onClick={() => window.location.reload()}
            >
              Reload
            </button>
          </div>
        </div>
      )
    }

    return this.props.children
  }
}

export default ErrorBoundary
